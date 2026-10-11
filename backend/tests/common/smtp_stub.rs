//! A local SMTP server that records every message it accepts, so a test can
//! count the mail the app really handed to SMTP, see who it went to and read
//! the link in it.
//!
//! Include it with `#[path = "common/smtp_stub.rs"] mod smtp_stub;` next to
//! `mod common;`. The server runs on a thread of its own: a `#[tokio::test]`
//! runtime ends with its test, and the stub has to outlive the test that
//! started it because the other tests in the binary share it.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

/// One message the server accepted.
#[derive(Clone, Debug)]
pub struct Delivered {
    pub rcpt: String,
    /// Everything between `DATA` and the closing dot, headers included.
    pub data: String,
}

impl Delivered {
    /// The message with its quoted-printable encoding undone.
    pub fn decoded(&self) -> String {
        decode_quoted_printable(&self.data)
    }

    pub fn subject(&self) -> String {
        self.data
            .lines()
            .find_map(|line| line.strip_prefix("Subject: "))
            .unwrap_or_default()
            .to_string()
    }

    /// The `From` header line.
    pub fn sender_header(&self) -> String {
        self.data
            .lines()
            .find_map(|line| line.strip_prefix("From: "))
            .unwrap_or_default()
            .to_string()
    }

    /// The sender's display name, whether the header carries it plain, quoted
    /// or as an RFC 2047 encoded word.
    pub fn sender_name(&self) -> String {
        use base64::Engine as _;
        let header = self.sender_header();
        let name = header.split(" <").next().unwrap_or_default();
        match name
            .strip_prefix("=?utf-8?b?")
            .and_then(|rest| rest.strip_suffix("?="))
        {
            Some(encoded) => String::from_utf8(
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .expect("base64 display name"),
            )
            .expect("utf-8 display name"),
            None => name.trim_matches('"').to_string(),
        }
    }

    /// The 64-character secret in the link the message carries.
    pub fn link_token(&self) -> String {
        let text = self.decoded();
        let marker = "token=";
        let start = text.find(marker).expect("a link with a token") + marker.len();
        text[start..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect()
    }
}

/// `=XX` escapes and `=` soft line breaks, as lettre writes the bodies.
pub fn decode_quoted_printable(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' {
            if bytes[i + 1..].starts_with(b"\r\n") {
                i += 3;
                continue;
            }
            if let Some(byte) = bytes
                .get(i + 1..i + 3)
                .and_then(|hex| std::str::from_utf8(hex).ok())
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Mail to one address that the server takes in but does not finish accepting
/// until [`MailHold::release`] is called, as a mail server that is slow or
/// unreachable would.
pub struct MailHold(Arc<Semaphore>);

impl MailHold {
    /// Let the held mail, and any later mail to the address, through.
    pub fn release(&self) {
        self.0.add_permits(1 << 20);
    }
}

#[derive(Clone)]
pub struct SmtpStub {
    pub port: u16,
    mails: Arc<Mutex<Vec<Delivered>>>,
    holds: Arc<Mutex<HashMap<String, Arc<Semaphore>>>>,
}

impl SmtpStub {
    /// Stall delivery to `address` until the returned hold is released. A
    /// request that waits for SMTP then never finishes, which a test can tell
    /// from one that does not wait, without measuring time.
    pub fn hold_mail_for(&self, address: &str) -> MailHold {
        let gate = Arc::new(Semaphore::new(0));
        self.holds
            .lock()
            .unwrap()
            .insert(address.to_lowercase(), gate.clone());
        MailHold(gate)
    }

    /// Every message accepted so far, oldest first.
    pub fn all(&self) -> Vec<Delivered> {
        self.mails.lock().unwrap().clone()
    }

    /// The messages addressed to `address`.
    pub fn to(&self, address: &str) -> Vec<Delivered> {
        self.all()
            .into_iter()
            .filter(|mail| mail.rcpt.eq_ignore_ascii_case(address))
            .collect()
    }

    /// Wait (up to five seconds) for `count` messages to `address`, for mail
    /// the app sends in a background task. Returns whatever arrived.
    pub async fn wait_for(&self, address: &str, count: usize) -> Vec<Delivered> {
        for _ in 0..100 {
            let mails = self.to(address);
            if mails.len() >= count {
                return mails;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        self.to(address)
    }
}

/// The process-wide stub. The first call starts it and points the SMTP
/// environment variables at it, so call it before building an `EmailService`.
pub fn smtp_stub() -> &'static SmtpStub {
    static STUB: OnceLock<SmtpStub> = OnceLock::new();
    STUB.get_or_init(|| {
        let mails: Arc<Mutex<Vec<Delivered>>> = Arc::default();
        let holds: Arc<Mutex<HashMap<String, Arc<Semaphore>>>> = Arc::default();
        let (port_tx, port_rx) = std::sync::mpsc::channel();
        let recorded = mails.clone();
        let held = holds.clone();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("stub runtime");
            runtime.block_on(async move {
                let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind stub");
                port_tx
                    .send(listener.local_addr().unwrap().port())
                    .expect("report stub port");
                loop {
                    let Ok((socket, _)) = listener.accept().await else {
                        return;
                    };
                    tokio::spawn(serve(socket, recorded.clone(), held.clone()));
                }
            });
        });
        let port = port_rx.recv().expect("stub port");

        crate::common::set_env("SMTP_HOST", "127.0.0.1");
        crate::common::set_env("SMTP_PORT", port.to_string());
        crate::common::set_env("SMTP_USER", "stub");
        crate::common::set_env("SMTP_PASS", "stub");
        crate::common::set_env("SMTP_TLS", "none");
        crate::common::set_env("SMTP_FROM_EMAIL", "noreply@mail.test");
        crate::common::set_env("FRONTEND_URL", "https://app.test");
        SmtpStub { port, mails, holds }
    })
}

async fn serve(
    socket: tokio::net::TcpStream,
    mails: Arc<Mutex<Vec<Delivered>>>,
    holds: Arc<Mutex<HashMap<String, Arc<Semaphore>>>>,
) {
    let (reader, mut writer) = socket.into_split();
    let mut reader = BufReader::new(reader);
    let _ = writer.write_all(b"220 stub ESMTP\r\n").await;
    let mut rcpt = String::new();
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let upper = line.trim_end().to_uppercase();
        let reply: &[u8] = if upper.starts_with("EHLO") || upper.starts_with("HELO") {
            b"250-stub\r\n250-AUTH PLAIN\r\n250 8BITMIME\r\n"
        } else if upper == "AUTH PLAIN" {
            let _ = writer.write_all(b"334 \r\n").await;
            line.clear();
            let _ = reader.read_line(&mut line).await;
            b"235 ok\r\n"
        } else if upper.starts_with("AUTH") {
            b"235 ok\r\n"
        } else if upper.starts_with("RCPT TO") {
            rcpt = line
                .split_once('<')
                .and_then(|(_, rest)| rest.split_once('>'))
                .map(|(address, _)| address.to_string())
                .unwrap_or_default();
            b"250 ok\r\n"
        } else if upper.starts_with("DATA") {
            let _ = writer.write_all(b"354 go\r\n").await;
            let mut data = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => return,
                    Ok(_) => {}
                }
                if line == ".\r\n" {
                    break;
                }
                data.push_str(&line);
            }
            let hold = holds.lock().unwrap().get(&rcpt.to_lowercase()).cloned();
            if let Some(gate) = hold {
                let _ = gate.acquire().await;
            }
            mails.lock().unwrap().push(Delivered {
                rcpt: std::mem::take(&mut rcpt),
                data,
            });
            b"250 queued\r\n"
        } else if upper.starts_with("QUIT") {
            let _ = writer.write_all(b"221 bye\r\n").await;
            return;
        } else {
            b"250 ok\r\n"
        };
        let _ = writer.write_all(reply).await;
    }
}
