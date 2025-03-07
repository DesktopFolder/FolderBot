use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufRead},
    path::Path,
};

use rand::{self, seq::SliceRandom};

use lazy_static::lazy_static;
use regex::Regex;

// This should be more generic in the future, but it works for now.
struct ResponseDB {
    responses: HashMap<String, Vec<String>>,
}

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

impl ResponseDB {
    fn new() -> ResponseDB {
        ResponseDB {
            responses: HashMap::new(),
        }
    }

    fn from_db(filename: &str) -> ResponseDB {
        lazy_static! {
            static ref CATEGORY_RE: Regex = Regex::new(r"^### *([\w_]+):\s*$").unwrap();
        }
        let mut ret = ResponseDB::new();
        let mut current_key = String::from("DEFAULT_KEY");
        for line in read_lines(filename).unwrap() {
            let line = line.unwrap();
            if line.trim().is_empty() {
                continue;
            }
            if line.starts_with("<!--") {
                continue;
            }
            let Some(caps) = CATEGORY_RE.captures(&line) else {
                ret.responses
                    .entry(current_key.clone())
                    .or_insert(Vec::new())
                    .push(line);
                continue;
            };
            current_key = String::from(&caps[1]);
        }
        ret
    }

    fn get(&self, key: &str) -> &Vec<String> {
        self.responses.get(key).unwrap()
    }
}

fn get_db(key: &'static str) -> &'static ResponseDB {
    lazy_static! {
        static ref DB: ResponseDB = ResponseDB::from_db("responses");
        static ref DEATHDB: ResponseDB = ResponseDB::from_db("resources/deaths.resp");
        static ref TITLES: ResponseDB = ResponseDB::from_db("resources/titles.resp");
    }
    match key {
        "main" => &DB,
        "deaths" => &DEATHDB,
        "titles" => &TITLES,
        _ => panic!("Bad key used: {}", key),
    }
}

pub fn random_response(key: &str) -> &'static String {
    get_db("main")
        .get(key)
        .choose(&mut rand::thread_rng())
        .unwrap()
}

pub fn has_responses(key: &str) -> bool {
    get_db("main").responses.contains_key(key)
}

pub fn db_random_response(key: &str, dbkey: &'static str) -> &'static String {
    get_db(dbkey)
        .get(key)
        .choose(&mut rand::thread_rng())
        .unwrap()
}
pub fn db_has_responses(key: &str, dbkey: &'static str) -> bool {
    get_db(dbkey).responses.contains_key(key)
}

pub fn file_greet_response(name: &str, files: i64) -> Option<String> {
    let getter = |s: &'static str| {
        if has_responses(s) {
            Some(
                random_response(s)
                    .replace("{ur}", name)
                    .replace("{fl}", files.to_string().as_ref()),
            )
        } else {
            println!("ERROR: {} has no response!", s);
            Some("[Internal Error]".to_string())
        }
    };
    match files {
        ..=1000 => None,
        2001..=5000 => getter("USER_GREET_1000"),
        7001..=10000 => getter("USER_GREET_5000"),
        11001..=25000 => getter("USER_GREET_10000"),
        25001..=50000 => getter("USER_GREET_25000"),
        50001..=10000000 => getter("USER_GREET_50000"),
        _ => None,
    }
}
