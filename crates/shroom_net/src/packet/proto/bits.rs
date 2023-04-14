use super::wrapped::PacketWrapped;
use bitflags::BitFlags;
use packed_struct::PackedStruct;

/// Wrapper around any BitFlags type, which allows En/Decoding of this type
pub struct ShroomBitFlags<T: BitFlags>(pub T);

impl<T: BitFlags> ShroomBitFlags<T> {
    pub fn new(inner: T) -> Self {
        Self(inner)
    }

    pub fn cloned(inner: &T) -> Self {
        Self(T::from_bits(inner.bits()).expect("bits"))
    }
}

impl<T> PacketWrapped for ShroomBitFlags<T>
where
    T: BitFlags,
{
    type Inner = T::Bits;

    fn packet_into_inner(&self) -> Self::Inner {
        self.0.bits()
    }

    fn packet_from(v: Self::Inner) -> Self {
        Self(T::from_bits_truncate(v))
    }
}

/// Mark the given `BitFlags` by implementing a Wrapper
/// The trait has to be explicitely implemented due to Trait rules
#[macro_export]
macro_rules! mark_shroom_bitflags {
    ($bits_ty:ty) => {
        impl $crate::packet::PacketWrapped for $bits_ty {
            type Inner = $crate::packet::ShroomBitFlags<$bits_ty>;

            fn packet_into_inner(&self) -> Self::Inner {
                Self::Inner::cloned(self)
            }

            fn packet_from(v: Self::Inner) -> Self {
                v.0
            }
        }
    };
}

/// Wrapper for `PacketStruct`
#[derive(Debug, PartialEq)]
pub struct ShroomPackedStruct<T: PackedStruct>(pub T);

impl<T> PacketWrapped for ShroomPackedStruct<T>
where
    T: PackedStruct + Clone,
{
    type Inner = T::ByteArray;

    fn packet_into_inner(&self) -> Self::Inner {
        self.0.pack().expect("pack")
    }

    fn packet_from(v: Self::Inner) -> Self {
        Self(T::unpack(&v).expect("unpack"))
    }
}

/// Mark the given `PacketStruct` by implementing a Wrapper
#[macro_export]
macro_rules! mark_shroom_packed_struct {
    ($packed_strct_ty:ty) => {
        impl $crate::packet::PacketWrapped for $packed_strct_ty {
            type Inner = $crate::packet::ShroomPackedStruct<$packed_strct_ty>;

            fn packet_into_inner(&self) -> Self::Inner {
                $crate::packet::ShroomPackedStruct(self.clone())
            }

            fn packet_from(v: Self::Inner) -> Self {
                v.0
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use bitflags::bitflags;

    use crate::packet::{proto::tests::{enc_dec_test_all, enc_dec_test}, ShroomPackedStruct};

    #[test]
    fn bits() {
        bitflags! {
            #[repr(transparent)]
            #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
            struct Flags: u32 {
                const A = 1;
                const B = 2;
                const C = 4;
            }
        }

        mark_shroom_bitflags!(Flags);

        enc_dec_test_all([Flags::A | Flags::B, Flags::all(), Flags::empty()]);
    }

    #[test]
    fn packet_struct() {
        use packed_struct::prelude::*;

        #[derive(PackedStruct, Clone, PartialEq, Debug)]
        #[packed_struct(bit_numbering = "msb0")]
        pub struct TestPack {
            #[packed_field(bits = "0..=2")]
            tiny_int: Integer<u8, packed_bits::Bits<3>>,
            #[packed_field(bits = "3")]
            enabled: bool,
            #[packed_field(bits = "4..=7")]
            tail: Integer<u8, packed_bits::Bits<4>>,
        }

        mark_shroom_packed_struct!(TestPack);

        enc_dec_test(TestPack {
            tiny_int: 5.into(),
            enabled: true,
            tail: 7.into()
        });

        enc_dec_test(ShroomPackedStruct(0u8));
    }
}
