use std::{io, iter};

use armor::{self, BlockType};
use composed::shared::Deserializable;
use composed::signed_key::{PublicOrSecret, SignedPublicKey, SignedSecretKey};
use errors::Result;
use packet::{Packet, PacketParser};
use types::Tag;

// TODO: can detect armored vs binary using a check if the first bit in the data is set. If it is cleared it is not a binary message, so can try to parse as armor ascii. (from gnupg source)

/// Parses a list of secret and public keys from ascii armored text.
pub fn from_armor_many<'a, R: io::Read + io::Seek + 'a>(
    input: R,
) -> Result<Box<dyn Iterator<Item = Result<PublicOrSecret>> + 'a>> {
    let mut dearmor = armor::Dearmor::new(input);
    dearmor.read_header()?;
    // Safe to unwrap, as read_header succeeded.
    let typ = dearmor
        .typ
        .ok_or_else(|| format_err!("dearmor failed to retrieve armor type"))?;

    // TODO: add typ and headers information to the key possibly?
    match typ {
        // Standard PGP types
        BlockType::PublicKey
        | BlockType::PrivateKey
        | BlockType::Message
        | BlockType::MultiPartMessage(_, _)
        | BlockType::Signature
        | BlockType::File => {
            // TODO: check that the result is what it actually said.
            Ok(from_bytes_many(dearmor))
        }
        BlockType::PublicKeyPKCS1
        | BlockType::PublicKeyPKCS8
        | BlockType::PublicKeyOpenssh
        | BlockType::PrivateKeyPKCS1
        | BlockType::PrivateKeyPKCS8
        | BlockType::PrivateKeyOpenssh => {
            unimplemented_err!("key format {:?}", typ);
        }
    }
}

/// Parses a list of secret and public keys from raw bytes.
pub fn from_bytes_many<'a>(
    bytes: impl io::Read + 'a,
) -> Box<dyn Iterator<Item = Result<PublicOrSecret>> + 'a> {
    let packets = PacketParser::new(bytes)
        .filter_map(|p| {
            // for now we are skipping any packets that we failed to parse
            if p.is_ok() {
                p.ok()
            } else {
                warn!("skipping packet: {:?}", p);
                None
            }
        })
        .peekable();

    Box::new(PubPrivIterator { inner: packets })
}

pub struct PubPrivIterator<I: Sized + Iterator<Item = Packet>> {
    inner: iter::Peekable<I>,
}

impl<I: Sized + Iterator<Item = Packet>> Iterator for PubPrivIterator<I> {
    type Item = Result<PublicOrSecret>;

    fn next(&mut self) -> Option<Self::Item> {
        let packets = self.inner.by_ref();
        if let Some(true) = packets.peek().map(|packet| packet.tag() == Tag::SecretKey) {
            let p: Option<Result<SignedSecretKey>> = SignedSecretKey::from_packets(packets).nth(0);
            p.map(|key| key.map(PublicOrSecret::Secret))
        } else if let Some(true) = packets.peek().map(|packet| packet.tag() == Tag::PublicKey) {
            let p: Option<Result<SignedPublicKey>> = SignedPublicKey::from_packets(packets).nth(0);
            p.map(|key| key.map(PublicOrSecret::Public))
        } else {
            None
        }
    }
}