pub mod filetime;
pub mod framed_pipe;
pub mod packet_buffer;

/// Helper type to calculate size hint
pub struct SizeHint(pub Option<usize>);

impl SizeHint {
    pub const ZERO: Self = Self::new(0);
    pub const NONE: Self = Self(None);

    pub const fn new(n: usize) -> Self {
        Self(Some(n))
    }

    /// Sum two Option<usize>
    /// When const traits become stable Add can be implemented
    pub const fn add(self, rhs: Self) -> Self {
        Self(match (self.0, rhs.0) {
            (Some(a), Some(b)) => Some(a + b),
            _ => None,
        })
    }

    /// Adds n to the value, If It is not None
    pub const fn add_n(self, rhs: usize) -> Self {
        match self.0 {
            Some(a) => Self::new(a + rhs),
            None => Self::NONE
        }
    }

    /// Multiply by n
    pub const fn mul_n(self, n: usize) -> Self {
        Self(match self.0 {
            Some(a) => Some(a * n),
            _ => None
        })
    }
}

pub const fn must_init_array_str<const CAP: usize>(init: &str) -> [u8; CAP] {
    let data = init.as_bytes();
    assert!(data.len() <= CAP);

    let mut s = [0; CAP];
    let mut i = 0;
    while i < data.len()  {
        s[i] = data[i];
        i += 1;
    }

    s
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const STR: [u8; 2] = must_init_array_str("AA");

    #[test]
    fn str_aa() {
        assert_eq!(STR.as_slice(), b"AA");
    }

    #[test]
    fn size_hint_add() {
        assert_eq!(SizeHint::ZERO.add(SizeHint(None)).0, None);
        assert_eq!(SizeHint::ZERO.add(SizeHint(Some(1))).0, Some(1));
    }

    #[test]
    fn size_hint_mul() {
        assert_eq!(SizeHint::ZERO.mul_n(0).0, Some(0));   
        assert_eq!(SizeHint::new(1).mul_n(2).0, Some(2));   
    }
}
