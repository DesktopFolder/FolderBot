use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::iter::Peekable;
use std::path::Path;
use std::str::Split;

/* CommandTree - A (strange) tree implementation.
 *
 * Goals of this data structure:
 *  - Lookup time should be more or less as fast as is reasonable
 *  - Lookups should be case insensitive (lazily for now)
 *  - Insertion and deletion time are nearly irrelevant
 *  - Lookup should return partial matches if there is no part of the search string that is 'wrong'
 *
 *  16/08 - Unlikely to follow through with most of this. Will probably use a KV store instead.
 *  Then can use trees for subcommands. Not a big deal to be missing autocomplete, & can still have
 *  case insensitivity (as lazy as it would have been before).
 *
 *  JSON should look like:
 *
 *  {
 *      "command": { "subcommands": { "x": { "value": CommandValue } }
 * haha yes this is very incomplete you're correct!
 */

// lol
fn get_true_lol() -> bool {
    true
}
fn is_false_lol(b: &bool) -> bool {
    !b
}
fn is_true_lol(b: &bool) -> bool {
    !!b
}
fn get_false_lol() -> bool {
    false
}
fn is_default_prefix(s: &String) -> bool {
    *s == default_prefix()
}
fn default_ver() -> String {
    "0.0.0".to_string()
}
fn default_host() -> String {
    "irc.chat.twitch.tv".to_string()
}
fn default_port() -> String {
    "6667".to_string()
}
fn default_prefix() -> String {
    "!".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub enum CmdValue {
    // We support string responses - eg "sayhi" => "hi!"
    StringResponse(String),
    // Aliases are automatically parsed and translated
    Alias(String),
    // Otherwise, it requires code implementation
    Generic(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommandNode {
    pub value: CmdValue,
    #[serde(skip_serializing_if = "is_false_lol", default = "get_false_lol")]
    pub admin_only: bool,
    // Only has an effect if admin_only is true
    // So it's fine to default this to true.
    // Mainly done that way for backwards compatibility.
    #[serde(skip_serializing_if = "is_true_lol", default = "get_true_lol")]
    pub super_only: bool,
    #[serde(skip_serializing_if = "HashMap::is_empty", default = "HashMap::new")]
    pub subcommands: HashMap<String, CommandNode>,
    // Anything with admin marked as true is auto-hidden
    #[serde(skip_serializing_if = "is_false_lol", default = "get_false_lol")]
    pub hidden: bool,
    #[serde(skip_serializing_if = "String::is_empty", default = "String::new")]
    pub sound: String,
    #[serde(skip_serializing_if = "is_default_prefix", default = "default_prefix")]
    pub prefix: String,
    #[serde(skip_serializing_if = "is_true_lol", default = "get_true_lol")]
    pub editable: bool,
}

impl CommandNode {
    pub fn new(value: CmdValue) -> CommandNode {
        CommandNode {
            value,
            admin_only: false,
            super_only: true,
            subcommands: HashMap::new(),
            hidden: false,
            sound: String::new(),
            prefix: default_prefix(),
            editable: true,
        }
    }

    pub fn with_prefix(mut self, prefix: String) -> CommandNode {
        if prefix != "" {
            self.prefix = prefix;
        }
        self
    }

    pub fn new_easter(value: CmdValue) -> CommandNode {
        CommandNode {
            value,
            admin_only: false,
            super_only: true,
            subcommands: HashMap::new(),
            hidden: true,
            sound: String::new(),
            prefix: default_prefix(),
            editable: true,
        }
    }

    pub fn new_private(value: CmdValue) -> CommandNode {
        CommandNode {
            value,
            admin_only: true,
            super_only: true,
            subcommands: HashMap::new(),
            hidden: true,
            sound: String::new(),
            prefix: default_prefix(),
            editable: true,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommandTree {
    #[serde(default = "default_ver")]
    version: String,
    #[serde(default = "default_host")]
    host: String,
    #[serde(default = "default_port")]
    port: String,
    #[serde(default = "HashMap::new")]
    commands: HashMap<String, CommandNode>,
    #[serde(default = "Vec::new")]
    pub admins: Vec<String>,
    #[serde(default = "String::new")]
    pub superuser: String,
}

impl CommandTree {
    pub fn find_subcommands<'a>(
        &self,
        itr: &mut Peekable<Split<char>>,
        node: &'a CommandNode,
    ) -> &'a CommandNode {
        // Now we find subcommands!
        match itr.peek() {
            Some(s) => {
                if *s == "--" {
                    let _ = itr.next();
                    return node;
                }
                let x: &[_] = &['-', '\n', '\r'];
                let sc = String::from(*(&s.trim_matches(x)));
                match node.subcommands.get(&sc) {
                    Some(n) => {
                        let _ = itr.next();
                        self.find_subcommands(itr, n)
                    }
                    None => node,
                }
            }
            None => node,
        }
    }

    pub fn find(&self, key: &mut String) -> Option<&CommandNode> {
        /* key is the full command string. For example:
         * "say hello, friends"
         * "wiw --start --timeout=20s functional!"
         * Therefore, we use itr as an iterator to the string,
         * which essentially returns sequential commands.
         */
        let mut itr = key.as_str().split(' ').peekable();
        let cmd = match itr.next() {
            Some(s) => s,
            None => return None,
        };
        /* after we're done, we do itr.collect::<Vec<String>>().join(' ') */
        match self.commands.get(&cmd.to_lowercase()) {
            Some(node) => match &node.value {
                CmdValue::Alias(k) => {
                    let mut h = HashSet::new();
                    h.insert(key.clone());
                    if let Some(res) = self.find_recurse(&k, h) {
                        // we have a mapping - res
                        // I've decided: No aliases for subcommands.
                        // In theory they support it, but I refuse to actually allow it.
                        let ret = Some(self.find_subcommands(&mut itr, res));
                        *key = String::from(itr.collect::<Vec<&str>>().join(" "));
                        ret
                    } else {
                        None
                    }
                }
                _ => {
                    let ret = Some(self.find_subcommands(&mut itr, node));
                    *key = String::from(itr.into_iter().collect::<Vec<&str>>().join(" "));
                    ret
                }
            },
            None => None,
        }
    }

    pub fn insert(&mut self, key: String, cmd: CommandNode) {
        self.commands.insert(key, cmd);
    }

    pub fn set_value(&mut self, key: &String, value: CmdValue) {
        self.get_always(key).value = value;
    }

    pub fn set_prefix(&mut self, key: &String, prefix: String) {
        self.get_always(key).prefix = prefix;
    }

    pub fn contains(&self, key: &String) -> bool {
        self.commands.contains_key(key)
    }

    pub fn get_always(&mut self, key: &String) -> &mut CommandNode {
        self.commands.get_mut(key).unwrap()
    }

    pub fn find_ref_mut(&mut self, key: &String) -> Option<&mut CommandNode>  {
        self.commands.get_mut(key)
    }

    pub fn find_recurse(&self, key: &String, mut prev: HashSet<String>) -> Option<&CommandNode> {
        match self.commands.get(key) {
            Some(node) => match &node.value {
                CmdValue::Alias(k) => {
                    if prev.insert(k.clone()) {
                        self.find_recurse(&k, prev)
                    } else {
                        None
                    }
                }
                _ => Some(node),
            },
            None => None,
        }
    }

    pub fn validate(ct: &CommandTree) -> bool {
        for (key, _value) in &ct.commands {
            for c in key.chars() {
                if c.is_uppercase() {
                    return false;
                }
            }
        }
        true
    }

    pub fn from_json_file(filename: &Path) -> CommandTree {
        let mut file =
            File::open(filename).expect(&format!("Could not open file: {}", filename.display()));
        let mut contents = String::new();
        file.read_to_string(&mut contents).expect(&format!(
            "Could not read file {} to string.",
            filename.display()
        ));
        CommandTree::from_json(serde_json::from_str(&contents).unwrap())
    }

    pub fn from_json(json: serde_json::Value) -> CommandTree {
        let mut ct: CommandTree = serde_json::from_value(json).unwrap();
        ct.commands.insert(
            String::from("rb:cancel"),
            CommandNode::new_private(CmdValue::Generic(String::from("internal:cancel"))),
        );
        ct
    }

    pub fn dump_file(&self, path: &Path) {
        serde_json::to_writer_pretty(&File::create(path).unwrap(), &self).unwrap()
    }

    pub fn setup_new(path: &Path) -> CommandTree {
        match path.exists() {
            // yes, it's a race condition
            // only overwrites though. not a big deal.
            true => panic!("Cannot setup new command tree; path already exists!"),
            false => {
                let mut ct = CommandTree {
                    commands: HashMap::new(),
                    version: default_ver(),
                    port: default_port(),
                    host: default_host(),
                    admins: Vec::new(),
                    superuser: String::new(),
                };
                ct.commands.insert("json".to_string(), 
                                   CommandNode::new_easter(
                                       CmdValue::StringResponse("The truth is alterable. The truth never has been altered. JSON is the best data format. JSON has always been the best data format.".to_string())));

                serde_json::to_writer_pretty(&File::create(path).unwrap(), &ct).unwrap();

                return ct;
            }
        }
    }
}
