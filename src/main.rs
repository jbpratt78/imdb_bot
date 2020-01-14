extern crate argparse;
extern crate env_logger;
extern crate imdb_index;
extern crate rusoto_core;
extern crate rusoto_s3;
extern crate serde_json;
extern crate ws;

use std::env;
use std::path::Path;
use std::{fs, result};

use argparse::{ArgumentParser, StoreTrue};
use failure;
use imdb_index::{Index, IndexBuilder, MediaEntity, Query, SearchResults, Searcher};
use rusoto_core::Region;
use rusoto_s3::S3Client;
use serde::Deserialize;
use url;
use ws::{connect, Handler, Handshake, Message, Request, Result, Sender};

mod download;

type ImdbResult<T> = result::Result<T, failure::Error>;

#[derive(Debug, Deserialize, PartialEq)]
struct Msg {
    nick: String,
    data: String,
}

struct Client {
    ws: Sender,
    s3_client: S3Client,
}

impl Handler for Client {
    fn build_request(&mut self, url: &url::Url) -> Result<Request> {
        let mut req = Request::from_url(url).unwrap();
        let key = "STRIMS_TOKEN";
        let val = env::var(key).unwrap();
        let cookie = format!("jwt={}", val);
        req.headers_mut().push(("Cookie".into(), cookie.into()));
        Ok(req)
    }

    fn on_open(&mut self, _: Handshake) -> Result<()> {
        // Now we don't need to call unwrap since `on_open` returns a `Result<()>`.
        // If this call fails, it will only result in this connection disconnecting.
        self.ws.send("Hello WebSocket")
    }

    fn on_message(&mut self, msg: Message) -> Result<()> {
        handle_rec(msg);
        Ok(())
    }
}

const IMDB_URL: &str = "https://www.imdb.com/title/";

fn main() {
    let _ = env_logger::try_init();

    let data_dir: &Path = Path::new("./data/");
    let index_dir: &Path = Path::new("./index/");
    let mut download = false;

    {
        // this block limits scope of borrows by ap.refer() method
        let mut ap = ArgumentParser::new();
        ap.set_description("Strims IMDB Bot");
        ap.refer(&mut download)
            .add_option(&["--download"], StoreTrue, "download imdb index files");
        ap.parse_args_or_exit();
    }

    if download {
        download::download_all(&data_dir).unwrap();
    }

    if !path_exists("./index") {
        println!("Building indices... This will take a while.");
        create_index(data_dir, index_dir).unwrap();
    }

    if let Err(error) = connect("wss://chat.strims.gg/ws", |ws| Client {
        ws,
        s3_client: S3Client::new(Region::UsWest1),
    }) {
        println!("Failed to create WebSocket due to: {:?}", error);
    }
}

fn path_exists(path: &str) -> bool {
    fs::metadata(path).is_ok()
}

// return string result if found then send from handler
fn handle_rec(msg: Message) -> () {
    match msg {
        Message::Text(text) => {
            let x = split_once(&text);
            match x[0] {
                "MSG" => {
                    let _v = match parse(x) {
                        Ok(v) => {
                            println!("{:?}", v);
                            if v.data.starts_with("!imdb") {
                                let x = v.data.trim_start_matches("!imdb");
                                let y = search_imdb(x);
                                let first_result = y.as_slice().first().unwrap().value();
                                //let first_result_rating = first_result.rating().unwrap();
                                println!(
                                    "Found: {} {}",
                                    first_result.title().title,
                                    //first_result_rating.rating,
                                    format!("{}{}", IMDB_URL, first_result.title().id)
                                );
                            }
                        }
                        Err(e) => panic!(e),
                    };
                }
                "JOIN" | "QUIT" => println!("join or quit: {}", x[1]),
                _ => println!("memes: {:?}", x),
            }
        }
        Message::Binary(_) => println!("weow binary msg received"),
    }
}

fn split_once(in_string: &str) -> Vec<&str> {
    in_string.splitn(2, ' ').collect()
}

fn parse(in_msg: Vec<&str>) -> ImdbResult<Msg> {
    let m: Msg = serde_json::from_str(in_msg[1])?;
    Ok(m)
}

fn search_imdb(query: &str) -> SearchResults<MediaEntity> {
    println!("starting search with {:}", query);
    let z: Query = Query::new().name(query);
    let data_dir: &Path = Path::new("./data/");
    let index_dir: &Path = Path::new("./index/");
    let opened_index = Index::open(data_dir, index_dir).unwrap();
    let mut searcher = Searcher::new(opened_index);
    searcher.search(&z).unwrap()
}

fn create_index(data_dir: &Path, index_dir: &Path) -> ImdbResult<Index> {
    Ok(IndexBuilder::new().create(data_dir, index_dir)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_once() {
        let in_msg: &str = "MSG {\"nick\":\"jbpratt\",\"features\":[],\"timestamp\":1568160987374,\"data\":\"test\"}";
        let out = vec![
            "MSG",
            "{\"nick\":\"jbpratt\",\"features\":[],\"timestamp\":1568160987374,\"data\":\"test\"}",
        ];
        assert_eq!(split_once(in_msg), out)
    }

    #[test]
    fn test_parse() {
        let out = Msg {
            nick: String::from("jbpratt"),
            data: String::from("test"),
        };
        let in_msg = vec![
            "MSG",
            "{\"nick\":\"jbpratt\",\"features\":[],\"timestamp\":1568160987374,\"data\":\"test\"}",
        ];
        assert_eq!(parse(in_msg).unwrap(), out)
    }
}
