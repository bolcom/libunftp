use bytes::Bytes;
use std::convert;
use std::fmt;

#[derive(PartialEq, Eq, Clone)]
pub struct Password {
    bytes: Bytes,
}

impl Password {
    pub fn new(bytes: Bytes) -> Self {
        Password { bytes }
    }
}

impl fmt::Display for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "*******")
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Password {{ bytes: ******* }}")
    }
}

impl convert::From<&str> for Password {
    fn from(s: &str) -> Self {
        Self::new(String::from(s).into())
    }
}

impl convert::AsRef<[u8]> for Password {
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const SECRET: &str = "supersecret";

    #[test]
    fn password_obsures_display() {
        assert_eq!("*******", format!("{}", password()));
    }

    #[test]
    fn password_obscures_debug() {
        assert_eq!("Password { bytes: ******* }", format!("{:?}", password()));
    }

    #[test]
    fn password_retrievable_as_ref() {
        assert_eq!(SECRET.as_bytes(), password().as_ref())
    }

    fn password() -> Password {
        Password::new(SECRET.into())
    }
}
