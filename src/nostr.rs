use esp_println::println;
use heapless::String;
use secp256k1::{self, ffi::types::AlignedType, KeyPair, Message};
use sha2::{Digest, Sha256};

pub enum NoteKinds {
    ShortNote = 1,
}

pub struct Note {
    id: [u8; 64],
    pubkey: [u8; 64],
    created_at: u8,
    kind: NoteKinds,
    content: String<64>,
    sig: [u8; 128],
}

impl Note {
    pub fn new(privkey: &str, content: &str) -> Self {
        println!("starting new note");
        let mut note = Note {
            id: [0; 64],
            pubkey: *b"098ef66bce60dd4cf10b4ae5949d1ec6dd777ddeb4bc49b47f97275a127a63cf",
            created_at: 1,
            kind: NoteKinds::ShortNote,
            content: content.into(),
            sig: [0; 128],
        };
        note.set_id();
        println!("getting sig");
        note.set_sig(privkey);
        println!("sig complete");
        note
    }

    fn to_hash_str(&self) -> [u8; 1536] {
        let mut hash_str = [0; 1536];
        let mut count = 0;
        "[0,".as_bytes().iter().for_each(|bs| {
            hash_str[count] = *bs;
            count += 1;
        });
        self.pubkey.iter().for_each(|bs| {
            hash_str[count] = *bs;
            count += 1;
        });
        ",".as_bytes().iter().for_each(|bs| {
            hash_str[count] = *bs;
            count += 1;
        });
        hash_str[count] = self.created_at;
        count += 1;
        ",".as_bytes().iter().for_each(|bs| {
            hash_str[count] = *bs;
            count += 1;
        });
        hash_str[count] = 4;
        count += 1;
        ",".as_bytes().iter().for_each(|bs| {
            hash_str[count] = *bs;
            count += 1;
        });
        count += 1;
        "[],".as_bytes().iter().for_each(|bs| {
            hash_str[count] = *bs;
            count += 1;
        });
        self.content.as_bytes().iter().for_each(|bs| {
            hash_str[count] = *bs;
            count += 1;
        });
        hash_str
    }

    fn set_id(&mut self) {
        let results = Sha256::digest(self.to_hash_str());
        base16ct::lower::encode(&results, &mut self.id).expect("encode error");
    }

    fn set_sig(&mut self, privkey: &str) {
        let mut buf = [AlignedType::zeroed(); 10_000];
        let sig_obj = secp256k1::Secp256k1::preallocated_new(&mut buf).unwrap();

        let message = Message::from_slice(&self.id[0..32]).expect("32 bytes");
        let key_pair = KeyPair::from_seckey_str(&sig_obj, privkey).expect("priv key failed");
        let sig = sig_obj.sign_schnorr_no_aux_rand(&message, &key_pair);
        base16ct::lower::encode(&sig.to_bytes(), &mut self.sig).expect("encode error");
    }

    fn to_json(&self) -> [u8; 1200] {
        let mut output = [0; 1200];
        let mut count = 0;
        r#"{"id": "#.as_bytes().iter().for_each(|bs| {
            output[count] = *bs;
            count += 1;
        });
        self.id.iter().for_each(|bs| {
            output[count] = *bs;
            count += 1;
        });
        r#","pubkey": "#.as_bytes().iter().for_each(|bs| {
            output[count] = *bs;
            count += 1;
        });
        self.pubkey.iter().for_each(|bs| {
            output[count] = *bs;
            count += 1;
        });
        r#","created_at": "#.as_bytes().iter().for_each(|bs| {
            output[count] = *bs;
            count += 1;
        });
        output[count] = self.created_at;
        count += 1;
        r#","kind": 1"#.as_bytes().iter().for_each(|bs| {
            output[count] = *bs;
            count += 1;
        });
        r#","tags": []"#.as_bytes().iter().for_each(|bs| {
            output[count] = *bs;
            count += 1;
        });
        r#","content": ""#.as_bytes().iter().for_each(|bs| {
            output[count] = *bs;
            count += 1;
        });
        self.content.as_bytes().iter().for_each(|bs| {
            output[count] = *bs;
            count += 1;
        });
        r#"","sig": "#.as_bytes().iter().for_each(|bs| {
            output[count] = *bs;
            count += 1;
        });
        self.sig.iter().for_each(|bs| {
            output[count] = *bs;
            count += 1;
        });
        r#"}"#.as_bytes().iter().for_each(|bs| {
            output[count] = *bs;
            count += 1;
        });

        output
    }

    pub fn to_relay(&mut self) -> [u8; 1535] {
        let mut output = [0; 1535];
        let mut count = 0;
        // fill in output
        r#"["EVENT", "#.as_bytes().iter().for_each(|bs| {
            output[count] = *bs;
            count += 1;
        });
        self.to_json().iter().for_each(|bs| {
            output[count] = *bs;
            count += 1;
        });
        r#"]\r\n\r\n"#.as_bytes().iter().for_each(|bs| {
            output[count] = *bs;
            count += 1;
        });

        output
    }
}
