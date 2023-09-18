use std::marker::PhantomData;

use crate::{
    DecodePacket, EncodePacket, PacketReader, PacketResult, PacketTryWrapped, PacketWriter,
    SizeHint,
};

pub trait PartialData<'de>: Sized {
    type Flags: bitflags::Flags;
    fn get_flags(&self) -> Self::Flags;
    fn partial_encode_packet<Buf: bytes::BufMut>(
        &self,
        flag: Self::Flags,
        pw: &mut PacketWriter<Buf>,
    ) -> PacketResult<()>;
    fn partial_decode_packet(flag: Self::Flags, pr: &mut PacketReader<'de>) -> PacketResult<Self>;
    fn partial_packet_len(&self, flag: Self::Flags) -> usize;
}

#[derive(Debug, Clone, PartialEq)]
pub struct AllFlags<Flags>(PhantomData<Flags>);

impl<Flags> Default for AllFlags<Flags> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<Flags: bitflags::Flags> PacketTryWrapped for AllFlags<Flags> {
    type Inner = Flags;

    fn packet_into_inner(&self) -> Self::Inner {
        Flags::all()
    }

    fn packet_try_from(_v: Self::Inner) -> PacketResult<Self> {
        //TODO maybe check the flags here
        Ok(Self::default())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartialFlag<Hdr, FlagData> {
    pub hdr: Hdr,
    pub data: FlagData,
}

impl<Hdr, FlagData> PartialFlag<Hdr, FlagData> {
    pub fn new(hdr: Hdr, data: FlagData) -> Self {
        Self { hdr, data }
    }
}

impl<FlagData> From<FlagData> for PartialFlag<(), FlagData> {
    fn from(value: FlagData) -> Self {
        Self::new((), value)
    }
}

impl<'de, Hdr, FlagData> EncodePacket for PartialFlag<Hdr, FlagData>
where
    Hdr: EncodePacket,
    FlagData: PartialData<'de>,
    FlagData::Flags: EncodePacket + std::fmt::Debug,
{
    const SIZE_HINT: SizeHint = SizeHint::NONE;

    fn packet_len(&self) -> usize {
        let flags = self.data.get_flags();
        flags.packet_len() + self.hdr.packet_len() + self.data.partial_packet_len(flags)
    }

    fn encode_packet<T: bytes::BufMut>(&self, pw: &mut PacketWriter<T>) -> PacketResult<()> {
        let flags = self.data.get_flags();
        self.data.get_flags().encode_packet(pw)?;
        self.hdr.encode_packet(pw)?;
        self.data.partial_encode_packet(flags, pw)?;

        Ok(())
    }
}

impl<'de, Hdr, FlagData> DecodePacket<'de> for PartialFlag<Hdr, FlagData>
where
    Hdr: DecodePacket<'de>,
    FlagData: PartialData<'de>,
    FlagData::Flags: DecodePacket<'de>,
{
    fn decode_packet(pr: &mut PacketReader<'de>) -> PacketResult<Self> {
        let flags = FlagData::Flags::decode_packet(pr)?;
        let hdr = Hdr::decode_packet(pr)?;
        let data = FlagData::partial_decode_packet(flags, pr)?;

        Ok(Self { hdr, data })
    }
}

#[macro_export]
macro_rules! partial_data {
    ($name:ident, $partial_name:ident, $partial_ty:ty, derive($($derive:ident),*), $($stat_name:ident($stat_ty:ty) => $stat_ix:expr),* $(,)?) => {
        bitflags::bitflags! {
            #[derive(Debug, Clone, Default)]
            pub struct $partial_name: $partial_ty {
                $(const $stat_name = $stat_ix;)*
            }
        }

        $crate::mark_shroom_bitflags!($partial_name);

        paste::paste! {
            impl $partial_name {
                $(pub fn [<has_ $stat_name:lower>](&self) -> bool {
                    self.contains(<$partial_name>::$stat_name)
                })*
            }


            #[derive($($derive),*)]
            pub struct [<$name Partial>] {
                $(
                    pub [<$stat_name:lower>]: $crate::CondOption<$stat_ty>,
                )*
            }

            impl <'de> $crate::proto::partial::PartialData<'de> for [<$name Partial>] {
                type Flags = $partial_name;

                fn get_flags(&self) -> Self::Flags {
                    let mut flags = $partial_name::empty();

                    $(
                        if self.[<$stat_name:lower>].is_some() {
                            flags  |= $partial_name::$stat_name;
                        }
                    )*;

                    flags
                }

                fn partial_encode_packet<Buf: bytes::BufMut>(&self, _flag: Self::Flags, pw: &mut $crate::PacketWriter<Buf>) -> $crate::PacketResult<()> {
                    use $crate::EncodePacket;
                    $(
                        self.[<$stat_name:lower>].encode_packet(pw)?;
                    )*
                    Ok(())
                }

                fn partial_decode_packet(flag: Self::Flags, pr: &mut $crate::PacketReader<'de>) -> $crate::PacketResult<Self> {
                    use $crate::proto::conditional::{CondOption, PacketConditional};
                    Ok(Self {
                        $([<$stat_name:lower>]: CondOption::<$stat_ty>::decode_packet_cond(
                                flag.contains(<$partial_name>::$stat_name),
                                pr
                            )?
                        ),*
                    })
                }

                fn partial_packet_len(&self, _flag: Self::Flags) -> usize {
                    use $crate::EncodePacket;
                    $(self.[<$stat_name:lower>].packet_len() +)*
                        0
                }
            }



            #[derive($($derive),*)]
            pub struct [<$name All>] {
                pub all_flags: $crate::proto::partial::AllFlags<$partial_name>,
                $(pub [<$stat_name:lower>]: $stat_ty,)*
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        partial::AllFlags,
        proto::{
            partial::{PartialData, PartialFlag},
            CondOption,
        },
        test_util::test_enc_dec,
    };

    #[test]
    fn test_simple() {
        partial_data!(
            TestStats,
            TestStatsFlags,
            u32,
            derive(Debug, Clone),
            A(u8) => 1 << 0,
            B(u16) => 1 << 1,
        );

        impl PartialEq for TestStatsAll {
            fn eq(&self, other: &Self) -> bool {
                self.a == other.a && self.b == other.b
            }
        }

        let _all = TestStatsAll {
            all_flags: AllFlags::default(),
            a: 1,
            b: 2,
        };

        let partial = TestStatsPartial {
            a: CondOption(None),
            b: CondOption(None),
        };

        let flags = partial.get_flags();
        assert!(!flags.has_a());
        assert!(!flags.has_b());

        //TODO: enc_dec_test(TestStatsAll::new(TestStatsAllData { a: 0xaa, b: 0x1234 }));

        impl PartialEq for TestStatsPartial {
            fn eq(&self, other: &Self) -> bool {
                self.a == other.a && self.b == other.b
            }
        }

        pub type TestPartialData = PartialFlag<(), TestStatsPartial>;
        test_enc_dec(TestPartialData::from(
            TestStatsPartial {
                a: None.into(),
                b: Some(0x1234).into(),
            }
            .clone(),
        ));
    }
}
