use euclid::{Point2D, Vector2D};

use crate::EncodePacket;

use super::{DecodePacketOwned, PacketWrapped};

impl<T, U> PacketWrapped for Vector2D<T, U>
where
    T: EncodePacket + DecodePacketOwned + Copy,
{
    type Inner = (T, T);

    fn packet_into_inner(&self) -> Self::Inner {
        (self.x, self.y)
    }

    fn packet_from(v: Self::Inner) -> Self {
        Self::from(v)
    }
}

impl<T, U> PacketWrapped for Point2D<T, U>
where
    T: EncodePacket + DecodePacketOwned + Copy,
{
    type Inner = Vector2D<T, U>;

    fn packet_into_inner(&self) -> Self::Inner {
        self.to_vector()
    }

    fn packet_from(v: Self::Inner) -> Self {
        v.to_point()
    }
}

#[cfg(test)]
mod tests {
    use euclid::default::Vector2D;

    use crate::packet::test_util::test_encode_decode_owned_all;

    #[test]
    fn vec_pt() {
        let v = [
            Vector2D::<u16>::new(1, 2),
            Vector2D::<u16>::new(1, 1),
            Vector2D::<u16>::new(2, 1)
        ];

        test_encode_decode_owned_all(v);
        test_encode_decode_owned_all(v.iter().map(|v| v.to_point()));
    }
}
