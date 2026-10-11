//! A software authenticator, so a test can act out the WebAuthn registration
//! and login ceremonies against the real handlers.
//!
//! The ceremonies cannot run headless in a browser, and a hand-written
//! credential blob in the database proves little: the server only treats a
//! passkey as real when it parses and verifies. This one holds a P-256 key,
//! builds the attestation (`none` format) and assertion a browser would send,
//! and signs with OpenSSL, the library webauthn-rs verifies with.
//!
//! Include it with `#[path = "common/soft_passkey.rs"] mod soft_passkey;` next
//! to `mod common;`. The server must be told its origin and RP ID first, see
//! [`ORIGIN`] and [`RP_ID`].
#![allow(dead_code)]

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use openssl::bn::{BigNum, BigNumContext};
use openssl::ec::{EcGroup, EcKey};
use openssl::ecdsa::EcdsaSig;
use openssl::nid::Nid;
use openssl::pkey::Private;
use openssl::sha::sha256;
use serde_json::{Value, json};

/// `FRONTEND_URL` for tests that use this authenticator (it is also the value
/// `smtp_stub` sets, so the two never disagree).
pub const ORIGIN: &str = "https://app.test";
/// `WEBAUTHN_RP_ID`: a parent of the origin's host, so a challenge that
/// advertises the host instead of the RP ID shows.
pub const RP_ID: &str = "test";

/// Point the server at [`ORIGIN`] and [`RP_ID`] and switch passkeys on.
pub fn pin_webauthn_environment() {
    crate::common::set_env("FRONTEND_URL", ORIGIN);
    crate::common::set_env("WEBAUTHN_RP_ID", RP_ID);
    crate::common::set_env("ENABLE_PASSKEYS", "true");
}

pub fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The first bytes of a CBOR item: major type and length.
fn cbor_head(major: u8, len: usize) -> Vec<u8> {
    match len {
        0..=23 => vec![major << 5 | len as u8],
        24..=255 => vec![major << 5 | 24, len as u8],
        _ => vec![major << 5 | 25, (len >> 8) as u8, len as u8],
    }
}

fn cbor_text(text: &str) -> Vec<u8> {
    [cbor_head(3, text.len()), text.as_bytes().to_vec()].concat()
}

fn cbor_bytes(bytes: &[u8]) -> Vec<u8> {
    [cbor_head(2, bytes.len()), bytes.to_vec()].concat()
}

pub struct SoftPasskey {
    key: EcKey<Private>,
    /// The credential id the authenticator reports.
    pub id: Vec<u8>,
    /// Signature counter of the last assertion.
    counter: u32,
}

impl SoftPasskey {
    pub fn new() -> Self {
        let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).expect("P-256");
        let key = EcKey::generate(&group).expect("generate key");
        let mut id = vec![0u8; 32];
        openssl::rand::rand_bytes(&mut id).expect("random id");
        Self {
            key,
            id,
            counter: 0,
        }
    }

    /// The credential id as the browser reports it in `PublicKeyCredential.id`.
    pub fn id_text(&self) -> String {
        b64(&self.id)
    }

    /// The public key as a COSE_Key (ES256, P-256).
    fn cose_key(&self) -> Vec<u8> {
        let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).expect("P-256");
        let mut ctx = BigNumContext::new().expect("bn context");
        let (mut x, mut y) = (BigNum::new().unwrap(), BigNum::new().unwrap());
        self.key
            .public_key()
            .affine_coordinates_gfp(&group, &mut x, &mut y, &mut ctx)
            .expect("coordinates");
        let mut cose = vec![0xa5, 0x01, 0x02, 0x03, 0x26, 0x20, 0x01, 0x21];
        cose.extend(cbor_bytes(&x.to_vec_padded(32).unwrap()));
        cose.push(0x22);
        cose.extend(cbor_bytes(&y.to_vec_padded(32).unwrap()));
        cose
    }

    fn client_data(kind: &str, options: &Value) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "type": kind,
            "challenge": options["publicKey"]["challenge"],
            "origin": ORIGIN,
            "crossOrigin": false,
        }))
        .expect("client data")
    }

    /// The `credential` a browser sends to `register/finish`, answering the
    /// `options` that `register/start` returned.
    pub fn registration(&self, options: &Value) -> Value {
        let client_data = Self::client_data("webauthn.create", options);
        let mut auth_data = sha256(RP_ID.as_bytes()).to_vec();
        // user present + user verified + attested credential data
        auth_data.push(0x45);
        auth_data.extend(0u32.to_be_bytes());
        auth_data.extend([0u8; 16]); // AAGUID
        auth_data.extend((self.id.len() as u16).to_be_bytes());
        auth_data.extend(&self.id);
        auth_data.extend(self.cose_key());

        let mut attestation = vec![0xa3];
        attestation.extend(cbor_text("fmt"));
        attestation.extend(cbor_text("none"));
        attestation.extend(cbor_text("attStmt"));
        attestation.push(0xa0);
        attestation.extend(cbor_text("authData"));
        attestation.extend(cbor_bytes(&auth_data));

        json!({
            "id": self.id_text(),
            "rawId": b64(&self.id),
            "response": {
                "attestationObject": b64(&attestation),
                "clientDataJSON": b64(&client_data),
            },
            "type": "public-key",
        })
    }

    /// The `credential` a browser sends to `login/finish`, answering the
    /// `options` that `login/start` returned. Every call counts one more use.
    pub fn assertion(&mut self, options: &Value) -> Value {
        self.counter += 1;
        let client_data = Self::client_data("webauthn.get", options);
        let mut auth_data = sha256(RP_ID.as_bytes()).to_vec();
        auth_data.push(0x05); // user present + user verified
        auth_data.extend(self.counter.to_be_bytes());

        let signed = [auth_data.clone(), sha256(&client_data).to_vec()].concat();
        let signature = EcdsaSig::sign(&sha256(&signed), &self.key)
            .expect("sign")
            .to_der()
            .expect("DER signature");

        json!({
            "id": self.id_text(),
            "rawId": b64(&self.id),
            "response": {
                "authenticatorData": b64(&auth_data),
                "clientDataJSON": b64(&client_data),
                "signature": b64(&signature),
                "userHandle": null,
            },
            "type": "public-key",
        })
    }
}

/// A credential that deserializes but verifies against nothing.
pub fn junk_assertion() -> Value {
    json!({
        "id": "AAAA",
        "rawId": "AAAA",
        "type": "public-key",
        "response": {
            "authenticatorData": "AAAA",
            "clientDataJSON": "AAAA",
            "signature": "AAAA",
            "userHandle": null,
        },
    })
}
