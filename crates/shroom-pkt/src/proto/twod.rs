use euclid::{Box2D, Point2D, Vector2D};

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

// Used as rectangle
impl<T, U> PacketWrapped for Box2D<T, U>
where
    T: EncodePacket + DecodePacketOwned + Copy,
{
    // x,y upper-left
    // x,y of lower-right
    type Inner = (Point2D<T, U>, Point2D<T, U>);

    fn packet_into_inner(&self) -> Self::Inner {
        (self.min, self.max)
    }

    fn packet_from(v: Self::Inner) -> Self {
        Self::new(v.0, v.1)
    }
}

#[cfg(test)]
mod tests {

    use euclid::{default::Vector2D, Box2D, Point2D, UnknownUnit};

    use crate::test_util::test_enc_dec_all;

    #[test]
    fn vec_pt() {
        let v = [
            Vector2D::<u16>::new(1, 2),
            Vector2D::<u16>::new(1, 1),
            Vector2D::<u16>::new(2, 1),
        ];

        test_enc_dec_all(v);
        test_enc_dec_all(v.iter().map(|v| v.to_point()));
    }

    #[test]
    fn boxes() {
        let b: [Box2D<u32, UnknownUnit>; 3] = [
            Box2D::new(Point2D::new(1, 2), Point2D::new(3, 4)),
            Box2D::new(Point2D::new(1, 1), Point2D::new(1, 1)),
            Box2D::new(Point2D::new(2, 1), Point2D::new(1, 1)),
        ];

        test_enc_dec_all(b);
    }
}
