/// Mark an enum which implements TryFromPrimitive and Into<Primitive>
/// as packet encode/decode-able
#[macro_export]
macro_rules! mark_shroom_enum {
    ($enum_ty:ty) => {
        impl $crate::PacketTryWrapped for $enum_ty {
            type Inner = <$enum_ty as num_enum::TryFromPrimitive>::Primitive;

            fn packet_try_from(v: Self::Inner) -> $crate::PacketResult<Self> {
                use num_enum::TryFromPrimitive;
                Ok(<$enum_ty>::try_from_primitive(v)?)
            }
            fn packet_into_inner(&self) -> Self::Inner {
                Self::Inner::from(self.clone())
            }
        }
    };
}

/// Define an enum with just numbers like:
/// shroom_enum_code!(EnumCode, u8, A = 1, B = 2, C = 3);
#[macro_export]
macro_rules! shroom_enum_code {
    // Without default
    ($name:ident, $repr_ty:ty, $($code_name:ident = $val:expr),+) => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, num_enum::TryFromPrimitive, num_enum::IntoPrimitive)]
        #[repr($repr_ty)]
        pub enum $name {
            $($code_name = $val,)*
        }

        $crate::mark_shroom_enum!($name);
    };

    // With default
    ($name:ident, $repr_ty:ty, default($def_name:ident = $def_val:expr), $($code_name:ident = $val:expr),+,) => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, num_enum::TryFromPrimitive, num_enum::IntoPrimitive, Default)]
        #[repr($repr_ty)]
        pub enum $name {
            #[default]
            $def_name = $def_val,
            $($code_name = $val,)*
        }

        $crate::mark_shroom_enum!($name);
    };
}

/// Create a packet enum type with variants likes:
///             #[derive(Debug, PartialEq)]
///             pub enum TestChoice: u16 {
///                 Zero(()) = 0,
///                 One(()) = 1,
///                 Two(u32) = 2,
///             }
#[macro_export]
macro_rules! shroom_pkt_enum {
    // More or less copied from the bit flags crate
    (
        $(#[$outer:meta])*
        $vis:vis enum $Enum:ident: $T:ty {
            $(
                $(#[$inner:ident $($args:tt)*])*
                $Variant:ident($VariantTy:ty) =  $VariantDisc:expr
            ),*
        }

        $($t:tt)*
    ) => {
        $(#[$outer])*
        #[repr($T)]
        $vis enum $Enum {
            $($Variant($VariantTy) = $VariantDisc),*
        }

        impl $crate::EncodePacket for $Enum {
            fn encode_packet<B: bytes::BufMut>(&self, pw: &mut $crate::PacketWriter<B>) -> $crate::PacketResult<()> {
                match self {
                    $(
                        Self::$Variant(v) => {
                            ($VariantDisc as $T).encode_packet(pw)?;
                            v.encode_packet(pw)?;
                        }
                    ),*
                }

                Ok(())

            }

            const SIZE_HINT: $crate::SizeHint = $crate::SizeHint::NONE;

            fn packet_len(&self) -> usize {
                match self {
                    $(
                        Self::$Variant(v) => {
                            <$T>::SIZE_HINT.0.expect("enum size") + v.packet_len()
                        }
                    ),*
                }
            }
        }

        impl<'de> $crate::DecodePacket<'de> for $Enum {
            fn decode_packet(pr: &mut $crate::PacketReader<'de>) -> $crate::PacketResult<Self> {
                let ix = <$T>::decode_packet(pr)?;
                Ok(match ix {
                    $(
                        $VariantDisc => {
                            let v = <$VariantTy>::decode_packet(pr)?;
                            Self::$Variant(v)
                        }
                    ),*
                    _ => return Err($crate::Error::InvalidEnumDiscriminant(ix as usize))
                })
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use crate::test_util::test_enc_dec_all;

    #[test]
    fn packet_enum() {
        shroom_pkt_enum!(
            #[derive(Debug, PartialEq)]
            pub enum TestChoice: u16 {
                One(()) = 0,
                Two(u32) = 2
            }
        );

        test_enc_dec_all([TestChoice::One(()), TestChoice::Two(1337)]);
    }

    #[test]
    fn enum_code() {
        shroom_enum_code!(Code, u8, A = 1, B = 2, C = 3);

        test_enc_dec_all([Code::A, Code::B, Code::C]);
    }
}
