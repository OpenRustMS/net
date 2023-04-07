#[macro_export]
macro_rules! shroom_constant {
    ($name:ident, $ty:ty, $val:expr) => {
        #[derive(Debug, Default, Clone, Copy)]
        pub struct $name;

        impl $crate::packet::PacketWrapped for $name {
            type Inner = $ty;

            fn packet_into_inner(&self) -> Self::Inner {
                $val
            }

            fn packet_from(_v: Self::Inner) -> Self {
                //TODO should the constant be verified?
                // maybe just in debug mode not sure yet
                /*if v != $val {
                    panic!("Invalid constant")
                }*/

                Self
            }
        }
    };
}

shroom_constant!(Zero32, u32, 0);