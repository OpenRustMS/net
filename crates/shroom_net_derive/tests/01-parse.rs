use either::Either;
use shroom_net_derive::ShroomPacket;

use shroom_net::{
    packet::conditional::{CondEither, CondOption},
    test_encode_decode, EncodePacket,
};

#[derive(ShroomPacket)]
pub struct Packet {
    name: u8,
    bitmask: u16,
}

#[derive(ShroomPacket)]
pub struct Packet2(u8, u16);

#[derive(Debug, Clone, Copy)]
#[repr(u16)]
pub enum TestOpcode {
    Action1 = 1,
}

impl From<TestOpcode> for u16 {
    fn from(val: TestOpcode) -> Self {
        val as u16
    }
}

impl TryFrom<u16> for TestOpcode {
    type Error = String;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(TestOpcode::Action1),
            _ => Err(format!("Invalid test opcode: {value}")),
        }
    }
}

#[derive(ShroomPacket, Debug, PartialEq, Eq)]
pub struct Packet3<'a> {
    name: &'a str,
    bitmask: u16,
}

fn check_name_even(name: &str) -> bool {
    name.len() % 2 == 0
}

#[derive(ShroomPacket, Debug, PartialEq, Eq)]
pub struct Packet4<'a, T> {
    name: &'a str,
    #[pkt(check(field = "name", cond = "check_name_even"))]
    bitmask: CondOption<u16>,
    val: T,
}

fn check_n_even(n: &u32) -> bool {
    n % 2 == 0
}

#[derive(ShroomPacket, Debug, PartialEq, Eq)]
pub struct Packet5 {
    n: u32,
    #[pkt(either(field = "n", cond = "check_n_even"))]
    either: CondEither<String, bool>,
}

#[derive(ShroomPacket, Debug, PartialEq, Eq)]
pub struct Packet6 {
    n: u32,
    #[pkt(size = "n")]
    data: Vec<u8>,
}

fn main() {
    assert_eq!(Packet::SIZE_HINT.0, Some(3));
    assert_eq!(Packet3::SIZE_HINT.0, None);

    test_encode_decode!(Packet3 {
        name: "aaa",
        bitmask: 1337,
    });

    test_encode_decode!(Packet4 {
        name: "aaa",
        bitmask: CondOption(None),
        val: 1337u16,
    });
    test_encode_decode!(Packet4 {
        name: "aaaa",
        bitmask: CondOption(Some(1337)),
        val: 1337u16,
    });

    test_encode_decode!(Packet5 {
        n: 2,
        either: CondEither(Either::Left("ABC".to_string()))
    });

    test_encode_decode!(Packet5 {
        n: 1,
        either: CondEither(Either::Right(false))
    });

    test_encode_decode!(Packet6 {
        n: 1,
        data: vec![0xaa]
    });
}
