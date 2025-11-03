use async_std::{
    // TODO use async_channel instead of unstable+slower
    channel::{Receiver, Sender},
    io::BufReader,
    net::TcpStream,
    prelude::*,
    task,
};
use async_trait::async_trait;
use futures::{select, FutureExt};
use itertools::Itertools;
use lazy_static::lazy_static;
use rand::{thread_rng, Rng};
use regex::Regex;
use std::sync::Mutex;
use std::time::Duration;
use std::time::SystemTime;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};
use std::{ops::Sub, path::Path, sync::atomic::AtomicI8};

use rspotify::model::{AdditionalType, PlayableItem};
use rspotify::prelude::*;

#[cfg(feature = "audio")]
use folderbot::audio::Audio;
use folderbot::commands::anyleaderboard::LeaderboardClient;
use folderbot::commands::mcsr::lookup;
use folderbot::db::game::GameState;
use folderbot::db::player::{Player, PlayerData, PlayerScratch};
use folderbot::enchants::roll_enchant;
use folderbot::game::Game;
use folderbot::responses::rare_trident;
use folderbot::spotify::SpotifyChecker;
use folderbot::trident::db_has_responses;
use folderbot::trident::{db_random_response, has_responses, random_response};
use folderbot::yahtzee::YahtzeeError;
use folderbot::{
    command_tree::{CmdValue, CommandNode, CommandTree},
    trident::file_greet_response,
};

use libretranslate::{translate_url, Language};
use std::time;
use surf::middleware::{Middleware, Next};
use surf::{Client, Request, Response, Result};

use serde::{Deserialize, Serialize};

// just stuff for libretranslate I guess
#[derive(Debug)]
pub struct Logger;

#[surf::utils::async_trait]
impl Middleware for Logger {
    async fn handle(&self, req: Request, client: Client, next: Next<'_>) -> Result<Response> {
        println!("sending request to {}: {:?}", req.url(), req);
        let now = time::Instant::now();
        let res = next.run(req, client).await?;
        println!("request completed ({:?})", now.elapsed());
        Ok(res)
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct MojangAPIResponse {
    name: String,
    id: String,
}

fn cur_time_or_0() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs()
}

fn has_been_n_seconds_since(n: u64, t: u64) -> bool {
    let ct = cur_time_or_0();
    ct > t + n
}

#[allow(dead_code)]
fn check_timer(dur: u64, last_time: u64) -> Option<u64> {
    let ct = cur_time_or_0();
    if ct > last_time + dur {
        Some(ct)
    } else {
        None
    }
}

// Temporary until I find the correct way to do this.
trait CaptureExt {
    fn str_at(&self, i: usize) -> String;
}

impl CaptureExt for regex::Captures<'_> {
    fn str_at(&self, i: usize) -> String {
        self.get(i).unwrap().as_str().to_string()
    }
}

fn bad_eval(s: String) -> String {
    lazy_static! {
        static ref EVAL_RE: Regex = Regex::new(r"\s*(\d+)\s*([+\-*/])\s*(\d+)").unwrap();
    }

    if let Some(caps) = EVAL_RE.captures(&s) {
        if let Ok(a) = caps.str_at(1).parse::<i64>() {
            if let Ok(b) = caps.str_at(3).parse::<i64>() {
                return match caps.get(2).unwrap().as_str() {
                    "*" => a
                        .checked_mul(b)
                        .map_or("Um... no, but nice try.".to_string(), |v| v.to_string()),
                    "/" => a
                        .checked_div(b)
                        .map_or("153. xD".to_string(), |v| v.to_string()),
                    "-" => a
                        .checked_sub(b)
                        .map_or("...why.".to_string(), |v| v.to_string()),
                    "+" => a
                        .checked_add(b)
                        .map_or("Great work, you rolled a 255!".to_string(), |v| {
                            v.to_string()
                        }),
                    _ => "Unknown...".to_string(),
                };
            }
        }
    }
    "Parse failure...".to_string()
}

fn trim_args_end(args: &str) -> &str {
    args.trim_end_matches(|c: char| !c.is_ascii() || c.is_whitespace()) // get random characters at end of messages sometimes
}

fn split_args(args: &str) -> Vec<&str> {
    args.split_whitespace().collect::<Vec<&str>>()
}

enum ReadResult {
    Stop(String),
    Continue(String),
}

/*
// Message filtering
enum FilterResult {
    Skip,
    Ban(String),
    Empty,
}

fn filter(_name: &String, message: &String) -> FilterResult {
    lazy_static! {
        static ref SPAM_RE_1: Regex =
            Regex::new(r"follower.{0,15}prime.{0,15}view.{0,25}bigfollows.{0,10}com").unwrap();
    }

    match SPAM_RE_1.captures(message.as_str()) {
        // TODO Use re search or something, this is offline code
        Some(_) => FilterResult::Ban(String::from("Your message has been marked as spam. To be unbanned, send a private message to DesktopFolder.")),
        _ => FilterResult::Empty,
    }
}
*/

enum Command {
    Stop,
    Continue,
}

struct IRCMessage(String);

#[async_trait]
trait IRCStream {
    async fn send(&mut self, text: IRCMessage) -> ();
}

// 0: No translation
// 10: 100% translation
static TRANSLATE_FRENCH: AtomicI8 = AtomicI8::new(1);

#[async_trait]
impl IRCStream for TcpStream {
    async fn send(&mut self, text: IRCMessage) {
        use rand::thread_rng;
        // 0..9.
        let i: i8 = thread_rng().gen_range(0..10);
        // TRANSLATE_FRENCH(0) => 10 => always < min, so we never translate
        let min: i8 = 10_i8
            .checked_sub(TRANSLATE_FRENCH.load(Ordering::Relaxed))
            .unwrap_or(10);
        let has_flag = text.0.ends_with("    \r\n");
        if !text.0.starts_with("PRIVMSG") || i < min || has_flag {
            println!("Sending: '{}'", text.0.trim());
            let _ = self.write(text.0.as_bytes()).await;
            return;
        }
        // lol
        let source = Language::English;
        let target = Language::French;

        // not bad
        let mut splitter = text.0.trim().splitn(2, ':');
        let first = splitter.next().unwrap();
        let second = splitter.next().unwrap();

        println!("Translating '{}' to French...", second);
        if let Ok(res) = translate_url(
            source,
            target,
            second.to_string(),
            "http://192.168.1.245:5000".to_string(),
            None,
        )
        .await
        {
            println!("Successfully translated.");
            let to_write = format!("{}:{}\r\n", first, res.output);
            println!("Translated to: '{}'", to_write.trim());
            let _ = self.write(to_write.as_bytes()).await;
        } else {
            println!("Translation failed.");
            let _ = self.write(text.0.as_bytes()).await;
        }
    }
}

struct TwitchFmt {}

impl TwitchFmt {
    fn pass(pass: &String) -> IRCMessage {
        IRCMessage(format!("PASS {}\r\n", pass))
    }
    fn nick(nick: &String) -> IRCMessage {
        IRCMessage(format!("NICK {}\r\n", nick))
    }
    fn join(join: &String) -> IRCMessage {
        IRCMessage(format!("JOIN #{}\r\n", join))
    }
    fn text(text: &String) -> IRCMessage {
        IRCMessage(format!("{}\r\n", text))
    }
    fn privmsg(text: &String, channel: &String) -> IRCMessage {
        IRCMessage(format!("PRIVMSG #{} :{}\r\n", channel, text))
    }
    fn pong() -> IRCMessage {
        IRCMessage("PONG :tmi.twitch.tv\r\n".to_string())
    }
}

struct IRCBotClient {
    nick: String,
    secret: String,
    reader: BufReader<TcpStream>,
    sender: Sender<IRCMessage>,
    channel: String,
    ct: CommandTree,
    game: Game,
    #[cfg(feature = "audio")]
    audio: Audio,
    autosave: bool,
    spotify: SpotifyChecker,
    player_data: PlayerData,
    any_leaderboard: Option<LeaderboardClient>,
    yahtzee: Option<folderbot::yahtzee::Yahtzee>,
}

// Class that receives messages, then sends them.
struct IRCBotMessageSender {
    writer: TcpStream,
    queue: Receiver<IRCMessage>,
}

impl IRCBotMessageSender {
    async fn launch_write(&mut self) {
        loop {
            match self.queue.recv().await {
                Ok(s) => {
                    self.writer.send(s).await;
                }
                Err(e) => {
                    println!("Uh oh, queue receive error: {}", e);
                    break;
                }
            }
            task::sleep(Duration::from_millis(200)).await;
        }
    }
}

impl IRCBotClient {
    async fn send_msg(&self, msg: String) {
        let _ = self
            .sender
            .send(TwitchFmt::privmsg(&msg, &self.channel))
            .await;
    }

    async fn connect(
        nick: String,
        secret: String,
        channel: String,
        ct: CommandTree,
    ) -> (IRCBotClient, IRCBotMessageSender) {
        // Creates the stream object that will go into the client.
        let stream = TcpStream::connect("irc.chat.twitch.tv:6667").await.unwrap();
        // Get a stream reference to use for reading.
        let reader = BufReader::new(stream.clone());
        let (s, r) = async_std::channel::unbounded(); // could use bounded(10) or sth
        (
            IRCBotClient {
                nick,
                secret,
                reader,
                sender: s,
                channel,
                ct,
                game: Game::new(),
                #[cfg(feature = "audio")]
                audio: Audio::new(),
                autosave: false,
                spotify: SpotifyChecker::new().await,
                player_data: PlayerData::new(),
                any_leaderboard: LeaderboardClient::new(),
                yahtzee: folderbot::yahtzee::Yahtzee::load_from_default_file(),
            },
            IRCBotMessageSender {
                writer: stream,
                queue: r,
            },
        )
        // return the async class for writing back down the TcpStream instead, which contains the
        // receiver + the tcpstream clone
    }

    async fn authenticate(&mut self) -> () {
        println!("Writing password...");
        let _ = self.sender.send(TwitchFmt::pass(&self.secret)).await;
        println!("Writing nickname...");
        let _ = self.sender.send(TwitchFmt::nick(&self.nick)).await;
        println!("Writing join command...");
        let _ = self.sender.send(TwitchFmt::join(&self.channel)).await;
    }

    /*
    async fn do_elevated(&mut self, mut cmd: String) -> Command {
        if cmd.starts_with("stop") {
            Command::Stop
        } else if cmd.starts_with("raw") {
            self.sender.send(cmd.split_off(4)).await;
            Command::Continue
        } else if cmd.starts_with("say") {
            self.privmsg(cmd.split_off(4)).await;
            Command::Continue
        } else {
            Command::Continue
        }
    }
    */

    async fn do_text_message(&mut self, user: String, cmd: String) -> Command {
        lazy_static! {
            static ref SCRATCH: std::sync::Mutex<HashMap<String, PlayerScratch>> =
                Mutex::new(HashMap::new());
            static ref STATE: Mutex<GameState> = Mutex::new(GameState {
                ..Default::default()
            });
        }
        let mut scratch = SCRATCH.lock().unwrap();
        let mut state = STATE.lock().unwrap();
        let messager = self.sender.clone();
        let channel = self.channel.clone();
        let pd: &mut Player = self.player_data.player(&user);
        state.last_message = cur_time_or_0();
        let send_msg = |msg: &String| {
            let msg = msg.clone();
            async move {
                match messager.send(TwitchFmt::privmsg(&msg, &channel)).await {
                    _ => {}
                };
            }
        };

        // Maybe greet.
        if scratch
            .entry(user.clone())
            .or_insert_with(|| PlayerScratch::new())
            .try_greet()
        {
            println!("Potentially greeting {}", &user);

            let mut response_mod = 5.0;
            if pd.files > 1000 {
                response_mod += 1.0;
            }
            if pd.files > 7000 {
                response_mod += 0.5;
            }
            if pd.files > 12000 {
                response_mod += 0.5;
            }

            // Generic greets only for now. Later, custom greets per player.
            // Ok, maybe we can do some custom greets.
            let ug = format!("USER_GREET_{}", &user);
            let name2 = pd.name().clone();
            if user == "pacmanmvc" && cmd.contains("opper") {
                send_msg(&"Good day, PacManner.".to_string()).await;
            } else if has_responses(&ug) && thread_rng().gen_bool(3.0 / response_mod) {
                println!("Sending USER user greet (3.0 / {}) for {}", response_mod, &user);
                let name = pd.name().clone();
                self.send_msg(random_response(&ug).replace("{ur}", &name))
                    .await;
            } else if thread_rng().gen_bool(1.0 / 10.0) {
                println!("Sending 1/10 GENERIC user greet for {}", &user);
                send_msg(&random_response("USER_GREET_GENERIC").replace("{ur}", &pd.name())).await;
            } else if let Some(file_resp) = file_greet_response(&name2, pd.files) {
                println!("Sending FILE user greet for {}", &user);
                self.send_msg(file_resp).await;
            } else {
                // scale this with messages sent or file count? lol kind of ties back into
                // reputation mechanism
                if thread_rng().gen_bool(1.0 / 3.0) {
                    println!("Sending GENERIC user greet for {}", &user);
                    send_msg(&random_response("USER_GREET_GENERIC").replace("{ur}", &pd.name()))
                        .await;
                } else {
                    println!("Failed 1/3 check for greet for {}", &user);
                }
            }
        } else if cmd.contains("linux")
            && !cmd.contains("kernel")
            && thread_rng().gen_bool(1.0 / 3.0)
        {
            send_msg(&String::from("Did you mean GNU/Linux? - Stallman")).await;
        }
        return Command::Continue;
    }

    async fn do_command(&mut self, user: String, mut prefix: String, mut cmd: String) -> Command {
        let format_str = format!("[Name({}),Command({})] Result: ", user, cmd);
        let log_res = |s| println!("{}{}", format_str, s);

        // user data <3
        let pd: &mut Player = self.player_data.player(&user);
        let messager = self.sender.clone();
        let channel = self.channel.clone();
        lazy_static! {
            static ref SCRATCH: std::sync::Mutex<HashMap<String, PlayerScratch>> =
                Mutex::new(HashMap::new());
            static ref STATE: Mutex<GameState> = Mutex::new(GameState {
                ..Default::default()
            });
        }
        // ensure this player exists

        // areweasyncyet? xd
        let send_msg = |msg: &String| {
            let msg = msg.clone();
            async move {
                match messager.send(TwitchFmt::privmsg(&msg, &channel)).await {
                    _ => {}
                };
            }
        };
        pd.sent_messages += 1;
        let tm = cur_time_or_0();
        if tm > (pd.last_message + /* 60s * 15m */ 60 * 15) {
            pd.last_message = tm;
            pd.files += 25;
        }

        // Compose the command
        // !todo -> prefix: !, cmd: todo
        // !!todo -> prefix: !!, cmd: todo
        // But, these need to map differently.
        // Recombine.
        if prefix == "folder " || prefix == "bot " {
            prefix = "!".to_string();
        }

        let (cmd_name, _) = cmd.split_at(cmd.find(' ').unwrap_or(cmd.len()));
        let cmd_name = cmd_name.to_string();

        // println!("cmd({}) prefix({})", cmd, prefix);

        let node = match self.ct.find(&mut cmd) {
            Some(x) => x,
            None => {
                log_res("Skipped as no match was found.");

                return self.do_text_message(user, cmd).await; // Not a valid command
            }
        };
        if prefix != node.prefix && !(prefix == "" && node.prefix == "^") {
            log_res("Skipped as prefix does not match.");
            return self.do_text_message(user, cmd).await;
        }

        pd.sent_commands += 1;

        let args = cmd;
        let mut scratch = SCRATCH.lock().unwrap();
        let mut state = STATE.lock().unwrap();
        println!("Arguments being returned -> '{}'", args);
        if node.admin_only
            && ((node.super_only && user != self.ct.superuser) || !(self.ct.admins.contains(&user)))
        {
            let _ = self
                .sender
                .send(TwitchFmt::privmsg(
                    &"Naughty naughty, that's not for you!".to_string(),
                    &self.channel,
                ))
                .await;
            log_res("Blocked as user is not bot administrator.");
            return Command::Continue;
        }
        let command = match &node.value {
            CmdValue::StringResponse(x) => {
                let _ = self
                    .sender
                    .send(TwitchFmt::privmsg(&x.clone(), &self.channel))
                    .await;
                log_res(format!("Returned a string response ({}).", x).as_str());
                #[cfg(feature = "audio")]
                if !node.sound.is_empty() {
                    // Maybe play a sound. But, let's not make this spammable.
                    if let Some(new_time) = check_timer(4, state.tm_sounds) {
                        self.audio.play_file(&node.sound);
                        state.tm_sounds = new_time;
                    }
                };
                return Command::Continue;
            }
            CmdValue::Alias(x) => {
                log_res(format!("! Didn't return an alias ({}).", x).as_str());
                return Command::Continue;
            }
            CmdValue::Generic(x) => {
                if x.as_str() == "debug:use_internal_mapping" {
                    &args
                } else {
                    x
                }
            }
        };

        macro_rules! reply_and_continue {
            ($e:expr) => {
                let _ = self
                    .sender
                    .send(TwitchFmt::privmsg($e, &self.channel))
                    .await;
                return Command::Continue;
            };
        }

        lazy_static! {
            static ref COMMAND_RE: Regex = Regex::new(r"^([^\s\w]?)(.*?)\s+(.+)$").unwrap();
        }

        // lol
        if let Some(death_time) = pd.death {
            let name = pd.name();
            if death_time + 15 + thread_rng().gen_range(0..=270) < cur_time_or_0() {
                pd.death = None;
                let _ = self
                    .sender
                    .send(TwitchFmt::privmsg(
                        &(db_random_response("RESURRECTION", "deaths").replace("{ur}", &name)),
                        &self.channel,
                    ))
                    .await;
            } else {
                if command == "feature:trident" {
                    self.send_msg(
                        db_random_response("DEAD_TRIDENT_ATTEMPT", "deaths").replace("{ur}", &name),
                    )
                    .await;
                    return Command::Continue;
                }
                self.send_msg(
                    db_random_response("DEAD_COMMAND_ATTEMPT", "deaths")
                        .replace("{ur}", &name)
                        .replace("{m.com}", &cmd_name),
                )
                .await;
                return Command::Continue;
            }
        }

        match command.as_str() {
            "meta:insert" | "meta:edit" => {
                // Let's ... try to get this to work I guess.
                let (mut newprefix, newcmdunc, newresp) = match COMMAND_RE.captures(args.as_str()) {
                    // there must be a better way...
                    Some(caps) => (caps.str_at(1), caps.str_at(2), caps.str_at(3)),
                    None => {
                        send_msg(
                            &"Nice try, but you have been thwarted by the command regex! Mwuahaha."
                                .to_string(),
                        )
                        .await;
                        return Command::Continue;
                    }
                };
                if newprefix == "" {
                    newprefix = "!".to_string();
                }
                let newcmd = (&newcmdunc.as_str()).to_lowercase();
                if newcmd != newcmdunc {
                    let _ = self
                        .sender
                        .send(TwitchFmt::privmsg(
                            &"Warning: Converting to case-insensitive.".to_string(),
                            &self.channel,
                        ))
                        .await;
                }

                if let Some(x) = self.ct.find(&mut newcmd.to_string()) {
                    if !x.editable {
                        let _ = self
                            .sender
                            .send(TwitchFmt::privmsg(
                                &"Command is not editable.".to_string(),
                                &self.channel,
                            ))
                            .await;
                        return Command::Continue;
                    }
                };

                let keycmd = newcmd.to_string();
                if self.ct.contains(&keycmd) {
                    if command != "meta:edit" {
                        let _ = self
                            .sender
                            .send(TwitchFmt::privmsg(
                                &"Command already exists. Use !edit instead.".to_string(),
                                &self.channel,
                            ))
                            .await;
                        return Command::Continue;
                    }
                    if let CmdValue::Generic(_) = self.ct.get_always(&keycmd).value {
                        let _ = self
                            .sender
                            .send(TwitchFmt::privmsg(
                                &"You cannot edit Generic commands.".to_string(),
                                &self.channel,
                            ))
                            .await;
                        return Command::Continue;
                    }
                    self.ct
                        .set_value(&keycmd, CmdValue::StringResponse(newresp.to_string()));
                    self.ct.set_prefix(&keycmd, newprefix.clone());
                    println!(
                        "New prefix: {}, new value: {} for keycmd: {}",
                        newprefix, newresp, keycmd
                    );
                    self.ct.dump_file(Path::new("commands.json"));
                } else {
                    self.ct.insert(
                        newcmd.to_string(),
                        CommandNode::new(CmdValue::StringResponse(newresp.to_string()))
                            .with_prefix(newprefix),
                    );
                    log_res("Saving commands to commands.json");
                    self.ct.dump_file(Path::new("commands.json"));
                }
            }
            "meta:isadmin" => self
                .sender
                .send(TwitchFmt::privmsg(
                    &format!("Status of {}: {}", args, self.ct.admins.contains(&args)),
                    &self.channel,
                ))
                .await
                .unwrap(),
            "meta:issuper" => self
                .sender
                .send(TwitchFmt::privmsg(
                    &format!("Status of {}: {}", args, self.ct.superuser == args),
                    &self.channel,
                ))
                .await
                .unwrap(),
            "meta:help" => self
                .sender
                .send(TwitchFmt::privmsg(
                    &"If you'd like to help me out, donate today using !donate".to_string(),
                    &self.channel,
                ))
                .await
                .unwrap(),
            "meta:stop" => {
                log_res("Stopping as requested by command.");
                return Command::Stop;
            }
            "admin:revive" => {
                let name = pd.name();
                if let Some(p) = self.player_data.apply(&args.to_lowercase(), |p| {
                    p.death = None;
                }) {
                    let othername = p.name();
                    let _ = self
                        .sender
                        .send(TwitchFmt::privmsg(
                            &(db_random_response("FAKE_RESURRECTION", "deaths")
                                .replace("{ur}", &name)
                                .replace("{otherur}", &othername)),
                            &self.channel,
                        ))
                        .await;
                }
                return Command::Continue;
            }
            "admin:derevive" => {
                let name = pd.name();
                if let Some(p) = self.player_data.apply(&args.to_lowercase(), |p| {
                    p.death = Some(cur_time_or_0());
                }) {
                    let othername = p.name();
                    let _ = self
                        .sender
                        .send(TwitchFmt::privmsg(
                            &(db_random_response("FAKE_DEATH", "deaths")
                                .replace("{ur}", &name)
                                .replace("{otherur}", &othername)),
                            &self.channel,
                        ))
                        .await;
                }
                return Command::Continue;
            }
            "meta:playerdata" => {
                let _ = self
                    .sender
                    .send(TwitchFmt::privmsg(
                        &format!(
                            "{}",
                            &self.player_data.player_or(&args.to_lowercase(), &user)
                        ),
                        &self.channel,
                    ))
                    .await
                    .unwrap();
            }
            "meta:say" => {
                log_res("Sent a privmsg.");
                let _ = self
                    .sender
                    .send(TwitchFmt::privmsg(&args, &self.channel))
                    .await;
            }
            "meta:say_raw" => {
                log_res("Send a raw message.");
                let _ = self.sender.send(TwitchFmt::text(&args)).await;
            }
            "meta:reload_commands" => {
                log_res("Reloaded commands from file.");
                self.ct = CommandTree::from_json_file(Path::new("commands.json"));
            }
            "meta:save_commands_test" => {
                log_res("Saving commands to commands.test.json");
                self.ct.dump_file(Path::new("commands.test.json"));
            }
            "meta:save_commands" => {
                log_res("Saving commands to commands.json");
                self.ct.dump_file(Path::new("commands.json"));
            }
            "meta:whois" => {
                let name = args.trim().to_lowercase();
                if name.is_empty() {
                    send_msg(&"Who's who? Where am I?".to_string()).await;
                    return Command::Continue;
                }

                let matches = self
                    .player_data
                    .players
                    .iter()
                    .filter(|e| {
                        e.0 == &name
                            || e.1
                                .nick
                                .as_ref()
                                .map(|n| n.to_lowercase() == name)
                                .unwrap_or_default()
                    })
                    .sorted_by_key(|e| e.0)
                    .map(|e| format!("{} ({})", e.1.name(), e.0))
                    .join(", ");

                if matches.is_empty() {
                    send_msg(&format!("There's no one called {} here folderSus", name)).await;
                } else {
                    let msg = if matches.len() <= 256 {
                        // actual limit is 500
                        matches
                    } else {
                        format!("{:.253}...", matches)
                    };
                    send_msg(&format!("Here's what I could find: {}", msg)).await;
                }
            }
            "game:bet_for" => {
                log_res("Bet that it works!");
                match self.game.bet_for(&user, &args) {
                    Err(e) => {
                        let _ = self
                            .sender
                            .send(TwitchFmt::privmsg(&e, &self.channel))
                            .await;
                    }
                    _ => {}
                }
            }
            "game:bet_against" => {
                log_res("Bet that it fails!");
                match self.game.bet_against(&user, &args) {
                    Err(e) => {
                        let _ = self
                            .sender
                            .send(TwitchFmt::privmsg(&e, &self.channel))
                            .await;
                    }
                    _ => {}
                }
            }
            "game:failed" => {
                log_res("Noted that it failed.");
                let _ = self
                    .sender
                    .send(TwitchFmt::privmsg(&self.game.failed(), &self.channel))
                    .await;
                if self.autosave {
                    self.game.save(); // Note: This should really be done in Game's code,
                                      // this is just a rushed impl
                }
            }
            "game:worked" => {
                log_res("Noted that it succeeded!");
                let _ = self
                    .sender
                    .send(TwitchFmt::privmsg(&self.game.worked(), &self.channel))
                    .await;
                if self.autosave {
                    self.game.save(); // Note: This should really be done in Game's code,
                                      // this is just a rushed impl
                }
            }
            "game:status" => {
                log_res("Returned a player's status.");
                let query = if args == "" { &user } else { &args };
                let _ = self
                    .sender
                    .send(TwitchFmt::privmsg(&self.game.status(query), &self.channel))
                    .await;
            }
            "game:reload" => {
                log_res("Reloaded the game.");
                self.game.reload();
            }
            "game:save" => {
                log_res("Saved the game.");
                self.game.save();
            }
            "game:autosave" => {
                log_res("Turned on autosave.");
                self.autosave = true;
            }
            "feature:translate" => {
                log_res("Translating a message.");
                let is_fr = match &args[..3] {
                    "fr " => true,
                    "en " => false,
                    _ => {
                        send_msg(
                            &"Error: Must start with either fr or en (target language)".to_string(),
                        )
                        .await;
                        return Command::Continue;
                    }
                };
                let source = args[3..].to_string();
                let to_lang = if is_fr {
                    Language::French
                } else {
                    Language::English
                };
                let from_lang = if !is_fr {
                    Language::French
                } else {
                    Language::English
                };

                if let Ok(res) = translate_url(
                    from_lang,
                    to_lang,
                    source,
                    "http://192.168.1.245:5000".to_string(),
                    None,
                )
                .await
                {
                    let to_write = format!("{}    ", res.output);
                    send_msg(&to_write).await;
                }
            }
            "feature:rsg" => {
                log_res("Printing what RSG does.");
                if let Ok(get_resp) = reqwest::get("http://shnenanigans.pythonanywhere.com/").await
                {
                    if let Ok(get_text) = get_resp.text().await {
                        if get_text.len() > 100 {
                            let _ = self
                                .sender
                                .send(TwitchFmt::privmsg(
                                    &String::from(
                                        "@shenaningans this command be broken again :sob:",
                                    ),
                                    &self.channel,
                                ))
                                .await;
                        } else {
                            let _ = self
                                .sender
                                .send(TwitchFmt::privmsg(&get_text, &self.channel))
                                .await;
                        }
                    }
                }
            }
            "feature:droptrident" => {
                send_msg(&random_response("TRIDENT_DROP").replace("{ur}", &pd.name())).await;
            }
            "feature:title" => {
                let s: &str = if db_has_responses(&args, "titles") {
                    &args
                } else {
                    "aa"
                };
                send_msg(&db_random_response(s, "titles")).await;
            }
            "feature:faketrident" => {
                send_msg(&random_response("FAKE_ROLL_TRIDENT").replace("{ur}", &pd.name())).await;
            }
            "feature:anylb" => {
                let p = match args.as_str() {
                    "trident" => |p: &Player| p.max_trident as i64,
                    "files" => |p: &Player| p.files,
                    "deaths" => |p: &Player| p.deaths as i64,
                    "messages" => |p: &Player| (p.sent_messages - p.sent_commands) as i64,
                    "commands" => |p: &Player| p.sent_commands as i64,
                    "rolled_tridents" => |p: &Player| p.tridents_rolled as i64,
                    "gunpowder" | "gp" => |p: &Player| p.best_gp as i64,
                    "d20" => |p: &Player| p.max_d20_rolled as i64,
                    "fumbles" => |p: &Player| p.min_d20_rolled as i64,
                    "yahtzee" => {
                        // hacky work around to not being able to capture self.yahtzee in the lambda
                        match self.yahtzee.as_ref() {
                            Some(y) => {
                                let lb = self
                                    .player_data
                                    .players
                                    .iter()
                                    .map(|e| (e.1.name(), y.get_total_yahtzees(e.0)))
                                    .filter(|t| t.1 > 0)
                                    .sorted_by(|a, b| b.1.cmp(&a.1))
                                    .take(10)
                                    .map(|t| format!("{}: {}", t.0, t.1))
                                    .join(", ");
                                if lb.is_empty() {
                                    let zayd_name = self
                                        .player_data
                                        .players
                                        .get(&"the_zayd".to_string())
                                        .map(|p| p.name())
                                        .unwrap_or("Zayd".to_string());
                                    reply_and_continue!(&format!("{}, probably", zayd_name));
                                }
                                reply_and_continue!(&lb);
                            }
                            None => return Command::Continue,
                        }
                    }
                    _ => return Command::Continue,
                };
                send_msg(&self.player_data.any_leaderboard(p)).await;
                return Command::Continue;
            }
            "feature:tridentpb" => {
                let _ = self
                    .sender
                    .send(TwitchFmt::privmsg(
                        &format!("{}'s trident pb is: {}", &user, pd.max_trident),
                        &self.channel,
                    ))
                    .await;
            }
            "feature:tridentlb" => {
                let lb = self.player_data.leaderboard();
                log_res(format!("Generated leaderboard: {}", &lb).as_str());
                let _ = self
                    .sender
                    .send(TwitchFmt::privmsg(
                        &format!("Trident Leaderboard: {}", &lb),
                        &self.channel,
                    ))
                    .await;
                return Command::Continue;
            }
            "feature:trident" => {
                pd.last_tridents.rotate_left(1);
                pd.last_tridents[4] = cur_time_or_0();

                // arg game preempt this command.
                if let Ok(pword) = args.parse::<u64>() {
                    if let Some(actual) = state.mainframe_password {
                        if pword == actual {
                            state.freed = Some(cur_time_or_0());
                            state.mainframe_password = None;
                        }
                    }
                }

                if let Some(freed) = state.freed {
                    if has_been_n_seconds_since(10, freed) && thread_rng().gen_bool(1.0 / 5.0) {
                        send_msg(&random_response("SHACKLE_BOT").replace("{ur}", &pd.name())).await;
                        state.freed = None;
                        return Command::Continue;
                    }
                    send_msg(&random_response("FREED_BOT").replace("{ur}", &pd.name())).await;
                    return Command::Continue;
                }

                // acc data
                pd.tridents_rolled += 1;
                let mut rng = thread_rng();
                let inner: i32 = rng.gen_range(0..=250);
                let res: i32 = {
                    let mut inner_res = rng.gen_range(0..=inner);
                    if user == "desktopfolder" && args.len() > 0 {
                        if let Ok(real_res) = args.parse::<i32>() {
                            inner_res = real_res;
                        }
                    }
                    inner_res
                };

                let restr = res.to_string();
                // res is your roll

                let is_pb = pd.max_trident < (res as u64);
                let _prev_pb = pd.max_trident;
                if is_pb {
                    pd.max_trident = res as u64;
                }

                let prev_roll = scratch
                    .entry(user.clone())
                    .or_insert_with(|| PlayerScratch::new())
                    .last_trident;
                scratch.get_mut(&user).unwrap().last_trident = res;

                pd.max_trident = std::cmp::max(pd.max_trident, res as u64);
                pd.trident_acc += res as u64;

                let name = pd.name();
                let norm_fmt = |s: &String| {
                    s.replace("{ur}", &name)
                        .replace("{t.r}", &restr)
                        .replace("{t.rolled}", &pd.tridents_rolled.to_string())
                };

                // SPECIFIC ROLLS - DO THESE FIRST, ALWAYS. It's just 250, lol.
                if res == 250 {
                    pd.rolled_250s += 1;
                    send_msg(&norm_fmt(random_response("TRIDENT_VALUE_250"))).await;
                    return Command::Continue;
                }

                // let's do a few things with this before we do anything crazy
                if is_pb && pd.tridents_rolled > 5
                /* don't overwrite 250 responses */
                {
                    send_msg(&norm_fmt(random_response("TRIDENT_PB_GENERIC"))).await;
                    return Command::Continue;
                }

                if pd.tridents_rolled <= 5 && res >= 100 {
                    send_msg(&norm_fmt(random_response("EARLY_HIGH_TRIDENT"))).await;
                    return Command::Continue;
                }

                if pd.tridents_rolled == 1 {
                    send_msg(&norm_fmt(random_response("FIRST_TRIDENT_GENERIC"))).await;
                    return Command::Continue;
                }

                if res < 5 && res == prev_roll {
                    send_msg(&norm_fmt(random_response("TRIDENT_DOUBLE_LOW"))).await;
                    return Command::Continue;
                }

                if !scratch.get_mut(&user).unwrap().try_dent() {
                    send_msg(&norm_fmt(random_response("TRIDENT_RATELIMIT_RESPONSE"))).await;
                    return Command::Continue;
                }

                // Game segment begin.
                if rng.gen_ratio(1 + (state.game_factor), 420 + (state.game_factor)) {
                    let val = state
                        .mainframe_password
                        .get_or_insert(rng.gen_range(100000..=999999));
                    send_msg(&norm_fmt(
                        &random_response("TRIDENT_MAINFRAME_HACK")
                            .replace("{mainframe_password}", &val.to_string()),
                    ))
                    .await;
                    state.game_factor = 0;
                    return Command::Continue;
                }
                state.game_factor += 1;
                // Game segment end.

                if res < 5 && rng.gen_bool(1.0 / 6.0) {
                    let deduction = rng.gen_range(12..32);
                    send_msg(&norm_fmt(&format!("Ew... a {{t.r}}. What a gross low roll, {{ur}}. I'm deducting {} files from you, just for that...", deduction))).await;
                    pd.files -= deduction;
                    return Command::Continue;
                }

                if res < 2 && rng.gen_bool(1.0 / 5.0) {
                    pd.deaths += 1;
                    pd.death = Some(cur_time_or_0());
                    send_msg(&norm_fmt(db_random_response("DEATH_LOW", "deaths"))).await;
                    return Command::Continue;
                }

                if res > 150 && res < 176 && rng.gen_bool(1.0 / 5.0) {
                    pd.deaths += 1;
                    pd.death = Some(cur_time_or_0());
                    send_msg(&norm_fmt(db_random_response("DEATH_HIGH", "deaths"))).await;
                    return Command::Continue;
                }

                let res_lookup = format!("TRIDENT_VALUE_RARE_{res}");
                if has_responses(&res_lookup) && rng.gen_bool(1.0 / 7.0) {
                    send_msg(&norm_fmt(random_response(&res_lookup))).await;
                    return Command::Continue;
                }

                if !has_been_n_seconds_since(10, state.last_message) {
                    // Spam prevention when people send messages.
                    if pd.last_tridents[4] != 0 && pd.last_tridents[4] - pd.last_tridents[0] < 5 {
                        // KILL KILL KILL
                        // uh I mean, yknow
                        pd.spam_prevention += 1;
                        pd.deaths += 1;
                        pd.death = Some(cur_time_or_0());
                        send_msg(&norm_fmt(db_random_response("DEATH_LOW", "deaths"))).await;
                        return Command::Continue;
                    }
                }

                if res < 66 && user == "pacmanmvc" && rng.gen_bool(1.0 / 10.0) {
                    let delta = 66 - res;
                    send_msg(&norm_fmt(&format!("{{t.r}}. Ouch. Just {delta} more, and you could have finished the TAS with that, eh \"Pac\" man? Whatever that means..."))).await;
                    return Command::Continue;
                }

                let selection = rng.gen_range(0..=100);
                if selection < 77 {
                    const LOSER_STRS: &'static [&'static str] = &["Wow, {} rolled a 0? What a loser!", "A 0... try again later, {} :/", "Oh look here, you rolled a 0. So sad! Alexa, play Despacito :sob:", "You rolled a 0. Everyone: Don't let {} play AA. They don't have the luck - er, skill - for it."];
                    const BAD_STRS: &'static [&'static str] = &["Hehe. A 1. So close, and yet so far, eh {}?", "{} rolled a 1. Everyone clap for {}. They deserve a little light in their life.", "A 1. Nice work, {}. I'm sure you did great in school.", "1. Do you know how likely that is, {}? You should ask PacManMVC. He has a spreadsheet, just to show how bad you are.", "Excuse me, officer? This 1-rolling loser {} keeps yelling 'roll trident!' at me and I can't get them to stop."];
                    const OK_STRS: &'static [&'static str] = &["{N}. Cool. That's not that bad.", "{N}! Wow, that's great! Last time, I rolled a 0, and everyone made fun of me :sob: I'm so jealous of you :sob:", "{N}... not terrible, I suppose.", "{N}. :/ <- That's all I have to say.", "{N}. Yeppers. Yep yep yep. Real good roll you got there, buddy.", "{N}! Whoa. A whole {N} more durability than 0, and you still won't get thunder, LOL!", "Cat fact cat fact! Did you know that the first {N} cats that spawn NEVER contain a Calico? ...seriously, where is my Calico??"];
                    const GOOD_STRS: &'static [&'static str] = &["{N}. Wow! I'm really impressed :)", "{N}! Cool, cool. Cool. Coooool.", "{N}... Hm. It's so good, and yet, really not that good.", "Here's a cat fact! Did you know they can eat up to {N} fish in a single day?!", "{N}. I lied about the cat fact, just FYI. I don't know anything about cats. He doesn't let me use the internet :(", "{N}. I want a cat. I'd treat it well and not abandon it in a random village.", "{N} temples checked before enchanted golden apple."];
                    const GREAT_STRS: &'static [&'static str] = &["{N}. Great work!!! That's going in your diary, I'm sure.", "{N}! Whoaaaaa. I'm in awe.", "{N}... Pretty great! You know what would be better? Getting outside ;) ;) ;)", "{N}. Oh boy! We got a high roller here!"];
                    if res == 0 {
                        let _ = self
                            .sender
                            .send(TwitchFmt::privmsg(
                                &LOSER_STRS[rng.gen_range(0..LOSER_STRS.len())]
                                    .replace("{}", &pd.name()),
                                &self.channel,
                            ))
                            .await;
                    } else if res == 1 {
                        let _ = self
                            .sender
                            .send(TwitchFmt::privmsg(
                                &BAD_STRS[rng.gen_range(0..BAD_STRS.len())]
                                    .replace("{}", &pd.name()),
                                &self.channel,
                            ))
                            .await;
                    } else if res < 100 {
                        let _ = self
                            .sender
                            .send(TwitchFmt::privmsg(
                                &OK_STRS[rng.gen_range(0..OK_STRS.len())].replace("{N}", &restr),
                                &self.channel,
                            ))
                            .await;
                    } else if res < 200 {
                        let _ = self
                            .sender
                            .send(TwitchFmt::privmsg(
                                &GOOD_STRS[rng.gen_range(0..GOOD_STRS.len())]
                                    .replace("{N}", &restr),
                                &self.channel,
                            ))
                            .await;
                    } else if res < 250 {
                        let _ = self
                            .sender
                            .send(TwitchFmt::privmsg(
                                &GREAT_STRS[rng.gen_range(0..GREAT_STRS.len())]
                                    .replace("{N}", &restr),
                                &self.channel,
                            ))
                            .await;
                    } else {
                        assert!(res == 250);
                        let _ = send_msg(&format!("You did it, {}! You rolled a perfect 250! NOW STOP SPAMMING MY CHAT, YOU NO LIFE TWITCH ADDICT!", &pd.name())).await;
                    }
                } else if selection < 82 && res != 250 {
                    send_msg(&norm_fmt(random_response("MISC_RARE_TRIDENTS"))).await;
                } else if selection < 85 && res < 10 {
                    send_msg(&norm_fmt(random_response("MISC_LOW_TRIDENTS"))).await;
                } else {
                    // ok, let's do this a bit better.
                    let _ = self
                        .sender
                        .send(TwitchFmt::privmsg(
                            &rare_trident(res, rng.gen_range(0..=4096), &pd.name()),
                            &self.channel,
                        ))
                        .await;
                }
            }
            "feature:tridentchance" => {
                let trimmed = args.trim();
                if trimmed.is_empty() {
                    return Command::Continue;
                }
                match trimmed.parse::<i64>().ok().filter(|n| *n >= 0 && *n <= 250) {
                    Some(n) => {
                        let mut odds: f64 = 0.0;
                        for k in n..=250 {
                            odds += 1.0 / (251.0 * (k + 1) as f64);
                        }

                        let chance: f64 = (1.0 / odds).ceil();

                        if chance == 63001.0 {
                            send_msg(&format!("You have a 1 in {} chance of rolling {}.. on the up side, if you round it, you have a 1 in 1 chance of not rolling {} monkaLaugh", chance, n, n)).await;
                        } else if chance > 5612.0 {
                            // 240 durability or more
                            send_msg(&format!("Rolling {} durability is a 1 in {} chance. Fun fact, you're twice as likely to get this than 250", n, chance)).await;
                        } else if chance > 1107.0 {
                            // 200 durability or more
                            send_msg(&format!("You have a 1 in {} chance of rolling {}. You have more of a chance of getting injured by a toilet OMEGALULiguess", chance, n)).await;
                        } else if chance > 488.0 {
                            // 150 durability or more
                            send_msg(&format!("You have a 1 in {} chance of {} durability, and yet still better odds than a calico spawning LULW", chance, n)).await;
                        } else if chance > 208.0 {
                            // 75 durability or more
                            send_msg(&format!("It's a 1 in {} chance of rolling {}. Did you know you have a higher chance of being born with an extra finger or toe?", chance, n)).await;
                        } else if chance > 109.0 {
                            // 25 durability or more
                            send_msg(&format!("You have a higher chance of falling to your death than the 1 in {} chance of rolling a {}", chance, n)).await;
                        } else {
                            // less than 25 durability
                            send_msg(&format!("There's a 1 in {} chance of rolling {} durability. It doesn't really get much better than that tbh. If you can't even roll a {} what's the point?", chance, n, n)).await;
                        }
                    }
                    None => {
                        send_msg(&format!(
                            "You might find it difficult to roll a {}, {}... but feel free to try",
                            trimmed,
                            &pd.name()
                        ))
                        .await;
                    }
                }
            }
            "feature:enchant" => {
                const ROMAN_MAP: &[&str] = &["I", "II", "III", "IV", "V"];
                const GREAT_ROLLS: &'static [&'static str] = &["Impressive! You've got yourself a {0} {1} book for {2} levels with {3} bookshel{4}.", "A truly magical outcome! {0} {1} awaits you for {2} levels with {3} bookshel{4}.", "Your enchantment game is strong! {0} {1} for you for the price of {2} levels. Not bad for {3} bookshel{4}.", "Surely you must be RNG-manipulating! I mean, {0} {1} for {2} levels!? I guess it did take {3} bookshel{4} to get."];
                const GOOD_ROLLS: &'static [&'static str] = &["{0} {1} from {3} bookshel{4}? Not too shabby! Yours for {2} levels.", "A respectable roll! Can't go wrong with {0} {1} for {2} levels with {3} bookshel{4}.", "{0} {1} for {2} levels. Could be worse, lol. I like your {3} bookshel{4}.", "Wow, not bad! {0} {1} for {2} levels with {3} bookshel{4}."];
                const BAD_ROLLS: &'static [&'static str] = &["{0} {1} for {2} levels? Could be worse, I guess... Might need more than {3} bookshel{4}...", "You rolled {0} {1} for {2} levels with {3} bookshel{4}. Keep trying!", "You rolled {0}! Nice!! Oh wait, its only {0} {1}. Oh well, it's only {2} levels at least. Maybe try using more than {3} bookshel{4} or something."];
                const TERRIBLE_ROLLS: &'static [&'static str] = &["{0}.. you know what. I can't be bothered telling you the level, it's too embarrassing. Let's just pretend it's a good level.", "Wow.. a {0} {1}.. amazing.. I wouldn't spend {2} levels on that, {5}.", "{0} {1}... zzz... something something {2} levels something {3} bookshel{4} idk I can't be bothered anymore", "Jackpot! You scored a {0} {1}. What are the odds of being that bad?? {2} levels?? Honestly. Get more bookshelves, {3} isn't enough.", "Yeah I'm not saying the response. That's just embarassing, {5}. Almost as embarassing as misspelling embarrassing."];
                match roll_enchant().filter(|o| o.level > 0 && (o.level as usize) < ROMAN_MAP.len())
                {
                    Some(offer) => {
                        pd.enchants_rolled += 1;
                        let response = if offer.special_response {
                            let resp_list = if offer.bookshelves >= 13 && offer.row == 3 {
                                GREAT_ROLLS
                            } else if offer.bookshelves >= 10 && offer.row > 1 {
                                GOOD_ROLLS
                            } else if offer.bookshelves < 2 {
                                TERRIBLE_ROLLS
                            } else {
                                BAD_ROLLS
                            };
                            resp_list[thread_rng().gen_range(0..resp_list.len())]
                                .replace("{0}", &offer.enchant.name)
                                .replace("{1}", ROMAN_MAP[offer.level as usize - 1])
                                .replace("{2}", &offer.cost.to_string())
                                .replace("{3}", &offer.bookshelves.to_string())
                                .replace("{4}", if offer.bookshelves == 1 { "f" } else { "ves" })
                                .replace("{5}", &pd.name())
                        } else {
                            format!(
                                "You rolled {0} {1} for {2} levels with {3} bookshel{4}!",
                                &offer.enchant.name,
                                ROMAN_MAP[offer.level as usize - 1],
                                offer.cost,
                                offer.bookshelves,
                                if offer.bookshelves == 1 { "f" } else { "ves" }
                            )
                        };
                        let _ = self
                            .sender
                            .send(TwitchFmt::privmsg(&response, &self.channel))
                            .await;
                    }
                    _ => {
                        let _ = self
                            .sender
                            .send(TwitchFmt::privmsg(
                                &"Somehow you rolled an impossible enchant... good for you"
                                    .to_string(),
                                &self.channel,
                            ))
                            .await;
                    }
                }
            }
            "feature:gunpowder" => {
                const ROLLS: u16 = 4 * 4; // 4 chests, 4 rolls each
                const CHANCE_PER_ROLL: f64 = 10.0 / 50.0;
                const MAX_GP: u64 = 8 * ROLLS as u64;

                let ps = scratch
                    .entry(user.clone())
                    .or_insert_with(|| PlayerScratch::new());

                // Limit 1 roll per 2 seconds - extend cooldown if another attempt is made (up to 60 seconds)
                if tm < ps.gp_ratelimit {
                    if ps.gp_ratelimit - tm < 60 {
                        ps.gp_ratelimit += 2;
                    }
                    return Command::Continue;
                }

                // Roll gunpowder
                let mut rng = thread_rng();
                let mut gp: u64 = 0;
                for _ in 0..ROLLS {
                    if rng.gen_bool(CHANCE_PER_ROLL) {
                        gp += rng.gen_range(1..=8);
                    }
                }

                // Stats collection
                pd.gp_rolled += 1;
                pd.gp_acc += gp;
                ps.gp_ratelimit = cur_time_or_0() + 2;
                if gp == MAX_GP {
                    pd.best_gp = gp;
                    pd.max_gp_rolled += 1;
                    send_msg(&format!("{} looted {} gunpowder!! folderWoah That's the maximum gunpowder you can loot! Well done!", pd.name(), gp)).await;
                } else if gp > pd.best_gp {
                    if pd.gp_rolled == 1 {
                        send_msg(&format!(
                            "{} received {} gunpowder from their first ever loot!",
                            pd.name(),
                            gp
                        ))
                        .await;
                    } else {
                        send_msg(&format!("{} looted {} gunpowder! PAGGING That's your new personal best! Your previous best was {} gunpowder.", pd.name(), gp, pd.best_gp)).await;
                    }
                    pd.best_gp = gp;
                } else if gp == 0 {
                    if rng.gen_bool(1.0 / 3.0) {
                        pd.deaths += 1;
                        pd.death = Some(cur_time_or_0());
                        match rng.gen_range(0..3) {
                            0 => send_msg(&format!("{} looted 0 gunpowder. monkaFlying They leap from the end ship with their new wings but forgot they didn't get gunpowder and hit the ground hard. RIP", pd.name())).await,
                            1 => send_msg(&format!("{} looted 0 gunpowder. RESETTING They rage quit and die from embarrassment.", pd.name())).await,
                            2 | _ => send_msg(&format!("{} looted 0 gunpowder. Feeling bad, a creeper approaches you offering gunpowd- oh nevermind. IMDEAD", pd.name())).await,
                        }
                    } else {
                        send_msg(&format!("{} looted 0 gunpowder. oof RESETTING", pd.name())).await;
                    }
                } else {
                    send_msg(&format!("{} looted {} gunpowder.", pd.name(), gp)).await;
                }
            }
            "feature:aaleaderboard" => {
                if self.any_leaderboard.is_none() {
                    println!("Leaderboard client not loaded");
                    return Command::Continue;
                }
                let lb = self.any_leaderboard.as_ref().unwrap();
                let trimmed_args = trim_args_end(&args);
                match lb.search(folderbot::commands::anyleaderboard::LeaderboardGameCategory::AllAdvancements, trimmed_args).await {
                    Ok(msg) => {
                        reply_and_continue!(&msg);
                    },
                    Err(_) => {
                        reply_and_continue!(&"Erm something went wrong".into());
                    }
                }
            }
            "feature:anypleaderboard" => {
                if self.any_leaderboard.is_none() {
                    println!("Leaderboard client not loaded");
                    return Command::Continue;
                }
                let lb = self.any_leaderboard.as_ref().unwrap();
                let trimmed_args = trim_args_end(&args);
                match lb
                    .search(
                        folderbot::commands::anyleaderboard::LeaderboardGameCategory::AnyPercent,
                        trimmed_args,
                    )
                    .await
                {
                    Ok(msg) => {
                        reply_and_continue!(&msg);
                    }
                    Err(_) => {
                        reply_and_continue!(&format!(
                            "Sorry, I don't know who {} is. smh",
                            trimmed_args
                        ));
                    }
                }
            }
            "feature:yahtzee" => {
                let yahtzee = match self.yahtzee.as_mut() {
                    Some(g) => g,
                    None => {
                        println!("Yahtzee game not loaded");
                        return Command::Continue;
                    }
                };
                let split_args = match trim_args_end(&args) {
                    "stats" => {
                        reply_and_continue!(&yahtzee.player_stats(&user));
                    }
                    "help" => {
                        reply_and_continue!(&"Roll all 5 dice with !yahtzee. You can re-roll up to two times by specifying the dice values you wish to save (e.g. !yahtzee 1 4). You only keep the scores that you don't re-roll. View stats with \"!yahtzee stats [name]\".".to_string());
                    }
                    "save" => {
                        if self.ct.admins.contains(&user) {
                            yahtzee.save()
                        }
                        return Command::Continue;
                    }
                    trimmed_args => split_args(&trimmed_args),
                };
                if split_args.get(0).map(|a| a == &"stats").unwrap_or_default() {
                    match split_args.get(1) {
                        Some(a) => {
                            reply_and_continue!(&yahtzee.player_stats(a));
                        }
                        None => return Command::Continue,
                    }
                }
                let saved = split_args
                    .iter()
                    .map(|arg| arg.parse::<u8>().ok())
                    .take_while(|n| n.filter(|n| *n > 0 && *n <= 6).is_some())
                    .map(|n| n.unwrap())
                    .collect::<Vec<_>>();
                if saved.len() >= folderbot::yahtzee::DICE_COUNT {
                    reply_and_continue!(&"That's too many dice MadgeJuice".to_string());
                }
                if saved.len() < split_args.len() {
                    reply_and_continue!(
                        &"Umm I don't think those are valid dice rolls majj".to_string()
                    );
                }
                let nick = pd.name();
                match yahtzee.play(&user, &saved) {
                    Ok(res) => {
                        reply_and_continue!(&res.replace("{ur}", &nick));
                    }
                    Err(err) => match err {
                        YahtzeeError::Private(reason) => println!("{}", &reason),
                        YahtzeeError::Public(display) => {
                            reply_and_continue!(&display.replace("{ur}", &nick));
                        }
                    },
                }
            }
            "feature:d20" => {
                let split_args = split_args(trim_args_end(&args));
                match split_args.get(0).map(|s| s.as_ref()) {
                    Some("stats") => {
                        let stats_user = match split_args.get(1) {
                            Some(a) => self.player_data.player_or(&a.to_string(), &user),
                            None => pd,
                        };
                        reply_and_continue!(&format!("d20 stats for {}: {} rolls, {} critical hits, {} fumbles, {:.2} average", stats_user.name(), stats_user.d20_rolled, stats_user.max_d20_rolled, stats_user.min_d20_rolled, stats_user.d20_acc as f64 / stats_user.d20_rolled as f64));
                    }
                    Some("info") => {
                        reply_and_continue!(&"Rolls a d20 die. Don't roll a 1, it could be deadly! Suggested by carl7879".to_string());
                    }
                    _ => {}
                }

                let mut rng = thread_rng();
                let roll = rng.gen_range(1..=20);
                pd.d20_rolled += 1;
                pd.d20_acc += roll as u64;

                let nick = pd.name();
                let response = match roll {
                    1 => {
                        pd.min_d20_rolled += 1;
                        pd.deaths += 1;
                        pd.death = Some(cur_time_or_0());
                        random_response("D20_FUMBLE")
                    }
                    20 => {
                        pd.max_d20_rolled += 1;
                        random_response("D20_CRIT_HIT")
                    }
                    _ => {
                        if rng.gen_bool(0.75) {
                            "You rolled a {roll}, {ur}"
                        } else {
                            random_response("D20_GENERIC")
                        }
                    }
                };
                send_msg(
                    &response
                        .replace("{roll}", &roll.to_string())
                        .replace("{ur}", &nick),
                )
                .await;
            }
            #[cfg(feature = "audio")]
            "admin:mute" => {
                self.audio.volume_default(0.0);
                return Command::Continue;
            }
            #[cfg(feature = "audio")]
            "admin:unmute" => {
                self.audio.volume_default(0.1);
                return Command::Continue;
            }
            "feature:nick" => {
                log_res("Setting nick");
                if args.len() > 0 {
                    pd.nick = Some(args);
                }
                send_msg(&random_response("NICK_SET").replace("{ur}", &pd.name())).await;
                return Command::Continue;
            }
            "feature:eval" => {
                send_msg(&format!("{} -> {}", args.clone(), bad_eval(args.clone()))).await;
                return Command::Continue;
            }
            "admin:nick" => {
                log_res("Setting nick (admin)");
                let v: Vec<&str> = args.splitn(2, "|").collect();
                if v.len() != 2 {
                    send_msg(&"Not enough arguments.".to_string()).await;
                    return Command::Continue;
                }
                let pde = self.player_data.player(&v[0].to_string());
                pde.nick = Some(v[1].to_string());
                return Command::Continue;
            }
            "admin:toggle_translate" => {
                log_res("Toggling translation mode.");
                if let Ok(i) = args.trim().parse::<i8>() {
                    if i <= 10 {
                        use std::sync::atomic::Ordering::Relaxed;
                        TRANSLATE_FRENCH.store(i, Relaxed);
                        return Command::Continue;
                    }
                }
                send_msg(&format!(
                    "{} is not a valid translation percentage. Must be 0..=10.",
                    &args
                ))
                .await;
            }
            "feature:elo" => {
                log_res("Doing elo things");
                self.send_msg(lookup(args).await).await;
                return Command::Continue;
            }
            "core:functioning_get_song" => {
                let song_response = self
                    .spotify
                    .spotify
                    .current_playing(None, Some([&AdditionalType::Track]))
                    .await;

                let message = match song_response {
                    Ok(playing) => match playing {
                        Some(playing) => match playing.item {
                            Some(playable_item) => match playable_item {
                                PlayableItem::Track(track) => {
                                    let artists = track.artists;

                                    let mut message = String::new();
                                    for (i, artist) in artists.iter().enumerate() {
                                        if i != artists.len() - 1 {
                                            message += &format!("{}, ", artist.name);
                                        } else {
                                            message += &format!("{} - ", artist.name);
                                        }
                                    }

                                    message += &track.name;
                                    message
                                }
                                _ => String::from(
                                    "no song, I'm just listening to Folding@Home podcast :)",
                                ),
                            },
                            None => String::from("Error: No song is currently playing."),
                        },
                        None => String::from("Error: No song is currently playing."),
                    },
                    Err(err) => {
                        println!("Error when getting the song: {:?}", err);
                        String::from("Error: Couldn't get the current song.")
                    }
                };

                let _ = self
                    .sender
                    .send(TwitchFmt::privmsg(&message, &self.channel))
                    .await;
            }
            #[cfg(feature = "audio")]
            "internal:cancel" => {
                self.audio.stop();
            }
            _ => {
                log_res("! Not yet equipped to handle this command.");
                return Command::Continue;
            }
        }
        log_res("Successfully executed command.");
        Command::Continue
    }

    async fn handle_twitch(&mut self, line: &String) -> Command {
        match line.trim() {
            "" => Command::Stop,
            "PING :tmi.twitch.tv" => {
                let _ = self.sender.send(TwitchFmt::pong()).await;
                Command::Continue
            }
            _ => Command::Continue,
        }
    }

    async fn launch_read(&mut self) -> ReadResult {
        lazy_static! {
            static ref COMMAND_RE: Regex =
                Regex::new(r"^(bot |folder |[^\s\w]|)\s*(.*?)\s*$").unwrap();
            static ref PRIV_RE: Regex =
                Regex::new(r":(\w*)!\w*@\w*\.tmi\.twitch\.tv PRIVMSG #\w* :\s*(.*)").unwrap();
        }
        let mut line = String::new();

        loop {
            line.clear();
            match self.reader.read_line(&mut line).await {
                Ok(_) => {
                    println!("[Received] Message: '{}'", line.trim());

                    // maybe save our game data real quick...
                    static LAST_SAVE: AtomicU64 = AtomicU64::new(0);

                    let tm = cur_time_or_0();
                    if (LAST_SAVE.load(Ordering::Relaxed) + 60 * 5) < tm {
                        LAST_SAVE.store(tm, Ordering::Relaxed);
                        println!("[Note] Autosaving player data.");
                        self.player_data.save();
                        if let Some(yahtzee) = &self.yahtzee {
                            yahtzee.save()
                        }
                    }

                    // First, parse if it's a private message, or a skip/ping/etc.
                    let (name, message) = match PRIV_RE.captures(line.as_str()) {
                        // there must be a better way...
                        Some(caps) => (caps.str_at(1), caps.str_at(2)),
                        None => match self.handle_twitch(&line).await {
                            // todo - reconnect instead of stopping.
                            Command::Stop => {
                                return ReadResult::Continue("Stopped due to twitch.".to_string())
                            }
                            _ => continue,
                        },
                    };

                    // Now we filter based on the username & the message sent.
                    //match filter(&name, &message) {
                    //    FilterResult::Skip => continue,
                    //    FilterResult::Ban(reason) => self.ban(&name, &reason).await,
                    //    _ => {}
                    //}

                    // Now, we parse the command out of the message.
                    let (prefix, command) = match COMMAND_RE.captures(message.as_str()) {
                        // there must be a better way...
                        Some(caps) => (caps.str_at(1), caps.str_at(2)),
                        // this never happens btw, we basically (?) always match (??)
                        None => continue,
                    };

                    // Finally, we actually take the command and maybe take action.
                    if let Command::Stop = self.do_command(name, prefix, command).await {
                        return ReadResult::Stop("Received stop command.".to_string());
                    }
                }
                Err(e) => {
                    println!("Encountered error: {}", e);
                    continue;
                }
            }
        }
    }
}

fn get_file_trimmed(filename: &str) -> String {
    match std::fs::read_to_string(filename) {
        Ok(s) => s.trim().to_string(),
        Err(e) => panic!("Could not open file ({}):\n{}", filename, e),
    }
}

async fn async_main() {
    let nick = get_file_trimmed("auth/user.txt");
    let secret = get_file_trimmed("auth/secret.txt");
    let channel = get_file_trimmed("auth/id.txt");
    // println!("Nick: {} | Secret: {} | Channel: {}", nick, secret, channel);

    loop {
        println!(
            "Connecting with nick '{}' to channel '{}' using auth/secret.txt",
            nick, channel
        );

        // Supported commands, loaded from JSON.
        let ct = CommandTree::from_json_file(Path::new("commands.json"));
        //ct.dump_file(Path::new("commands.parsed.json"));
        let (mut client, mut forwarder) =
            IRCBotClient::connect(nick.clone(), secret.clone(), channel.clone(), ct).await;
        client.authenticate().await;

        select! {
            return_message = client.launch_read().fuse() => match return_message {
                ReadResult::Continue(message) => { println!("Continuing (restarting) (Read): {}", message); },
                ReadResult::Stop(message) => { println!("Stopping (Read): {}", message); break; },
            },
            () = forwarder.launch_write().fuse() => {}
        }
        task::sleep(Duration::from_millis(5000)).await;
    }
}

fn main() {
    //println!("{}", rare_trident(17, 0, &String::from("hi")));
    //println!("{}", rare_trident(17, 0, &String::from("hi")));
    task::block_on(async_main())
}
