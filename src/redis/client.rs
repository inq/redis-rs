extern mod extra;

use std::vec::bytes::push_bytes;
use std::io::Reader;
use std::io::Writer;
use std::io::net::ip::SocketAddr;
use std::io::net::get_host_addresses;
use std::io::net::tcp::TcpStream;
use std::from_str::from_str;
use std::str::{from_utf8, from_utf8_owned};
use extra::url::Url;

use parser::Parser;
use parser::ByteIterator;

use enums::*;

mod macros;

pub enum ConnectFailure {
    InvalidURI,
    HostNotFound,
    ConnectionRefused,
}


fn value_to_string_list(val: &Value) -> ~[~str] {
    match *val {
        Bulk(ref items) => {
            let mut rv = ~[];
            for item in items.iter() {
                match item {
                    &Data(ref payload) => {
                        rv.push(from_utf8(*payload).to_owned());
                    },
                    _ => {}
                }
            }
            rv
        },
        _ => ~[],
    }
}


pub enum CmdArg<'a> {
    StrArg(&'a str),
    IntArg(int),
    BytesArg(&'a [u8]),
}

pub struct Client {
    priv addr: SocketAddr,
    priv sock: TcpStream,
    priv db: uint,
}

impl Client {

    /// opens a connection to redis by URI
    pub fn open(uri: &str) -> Result<Client, ConnectFailure> {
        let parsed_uri = try_unwrap!(from_str::<Url>(uri), Err(InvalidURI));
        let ip_addrs = try_unwrap!(get_host_addresses(parsed_uri.host), Err(InvalidURI));
        let ip_addr = try_unwrap!(ip_addrs.iter().next(), Err(HostNotFound));
        let port = try_unwrap!(from_str::<u16>(parsed_uri.port.clone()
            .unwrap_or(~"6379")), Err(InvalidURI));
        let db = from_str::<uint>(parsed_uri.path.trim_chars(&'/')).unwrap_or(0);

        let addr = SocketAddr {
            ip: *ip_addr,
            port: port
        };
        let sock = try_unwrap!(TcpStream::connect(addr), Err(ConnectionRefused));

        let mut rv = Client {
            addr: addr,
            sock: sock,
            db: 0,
        };

        if (db != 0) {
            rv.select_db(db);
        }

        Ok(rv)
    }

    /// executes a low-level redis command
    pub fn execute(&mut self, args: &[CmdArg]) -> Value {
        self.send_command(args);
        self.read_response()
    }

    fn pack_command(&self, args: &[CmdArg]) -> ~[u8] {
        let mut rv = ~[];
        push_byte_format!(&mut rv, "*{}\r\n", args.len());

        for arg in args.iter() {
            let mut buf;
            let encoded_arg = match arg {
                &StrArg(s) => s.as_bytes(),
                &IntArg(i) => {
                    let i_str = i.to_str();
                    buf = i_str.as_bytes().to_owned();
                    buf.as_slice()
                },
                &BytesArg(b) => b,
            };
            push_byte_format!(&mut rv, "${}\r\n", encoded_arg.len());
            push_bytes(&mut rv, encoded_arg);
            push_bytes(&mut rv, bytes!("\r\n"));
        }

        rv
    }

    fn send_command(&mut self, args: &[CmdArg]) {
        let cmd = self.pack_command(args);
        let w = &mut self.sock as &mut Writer;
        w.write(cmd);
    }

    fn read_response(&mut self) -> Value {
        let mut parser = Parser::new(ByteIterator {
            reader: &mut self.sock as &mut Reader,
        });
        parser.parse_value()
    }

    // commands

    pub fn select_db(&mut self, db: uint) -> bool {
        match self.execute([StrArg("SELECT"), IntArg(db as int)]) {
            Success => { self.db = db; true },
            _ => false,
        }
    }

    pub fn ping(&mut self) -> bool {
        match self.execute([StrArg("PING")]) {
            Status(~"PONG") => true,
            _ => false,
        }
    }

    pub fn keys(&mut self, pattern: &str) -> ~[~str] {
        let resp = self.execute([StrArg("KEYS"), StrArg(pattern)]);
        value_to_string_list(&resp)
    }

    pub fn get_bytes(&mut self, key: &str) -> Option<~[u8]> {
        match self.execute([StrArg("GET"), StrArg(key)]) {
            Data(value) => Some(value),
            _ => None,
        }
    }

    pub fn get(&mut self, key: &str) -> Option<~str> {
        match self.get_bytes(key) {
            None => None,
            Some(x) => Some(from_utf8_owned(x)),
        }
    }

    pub fn get_as<T: FromStr>(&mut self, key: &str) -> Option<T> {
        match self.get(key) {
            None => None,
            Some(x) => from_str(x),
        }
    }

    pub fn set_bytes(&mut self, key: &str, value: &[u8]) -> bool {
        match self.execute([StrArg("SET"), StrArg(key), BytesArg(value)]) {
            Success => true,
            _ => false,
        }
    }

    pub fn set<T: ToStr>(&mut self, key: &str, value: T) -> bool {
        let v = value.to_str();
        match self.execute([StrArg("SET"), StrArg(key), StrArg(v)]) {
            Success => true,
            _ => false,
        }
    }
}
