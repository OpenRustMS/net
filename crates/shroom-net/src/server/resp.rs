use async_trait::async_trait;
use shroom_pkt::{
    opcode::{HasOpcode, NetOpcode},
    EncodePacket,
};

use crate::{codec::ShroomCodec, NetResult, ShroomConn};

//TODO: either remove async_trait for performance reasons
// or wait for async fn's in trait becoming stable

/// Represents a response which can be sent with the session
/// Returning a ConnHandleResult
#[async_trait]
pub trait Response {
    async fn send<C: ShroomCodec>(self, session: &mut ShroomConn<C>) -> NetResult<()>;
}

#[async_trait]
impl Response for () {
    async fn send<C: ShroomCodec>(self, _session: &mut ShroomConn<C>) -> NetResult<()> {
        Ok(())
    }
}

/// Sending the value Some value If It's set
#[async_trait]
impl<Resp: Response + Send> Response for Option<Resp> {
    async fn send<C: ShroomCodec>(self, session: &mut ShroomConn<C>) -> NetResult<()> {
        match self {
            Some(resp) => resp.send(session).await,
            None => Ok(()),
        }
    }
}

/// Sending all Responses in this `Vec`
#[async_trait]
impl<Resp: Response + Send> Response for Vec<Resp> {
    async fn send<C: ShroomCodec>(self, session: &mut ShroomConn<C>) -> NetResult<()> {
        for resp in self.into_iter() {
            resp.send(session).await?;
        }
        Ok(())
    }
}

/// Represents a Response Packet, which stores the Data and the Opcode
pub struct ResponsePacket<T> {
    pub op: u16,
    pub data: T,
}

/// Normal Packet with Encode and Opcode
impl<T: EncodePacket + HasOpcode> From<T> for ResponsePacket<T> {
    fn from(value: T) -> Self {
        ResponsePacket::new(T::OPCODE, value)
    }
}

impl<T> ResponsePacket<T> {
    /// Creates a new response packet from the data `T` which is supposed to implement EncodePacket
    /// and the given Opcode
    pub fn new(op: impl NetOpcode, data: T) -> Self {
        Self {
            op: op.into(),
            data,
        }
    }
}

/// Simply send the packet with the opcode over the session
#[async_trait]
impl<T> Response for ResponsePacket<T>
where
    T: EncodePacket + Send,
{
    async fn send<C: ShroomCodec>(self, session: &mut ShroomConn<C>) -> NetResult<()> {
        session
            .send_encode_packet_with_opcode(self.op, self.data)
            .await?;
        Ok(())
    }
}

/// Helper trait which allows to transform and encode-able type
/// and an Opcode into a Response Packet
pub trait PacketOpcodeExt: EncodePacket {
    fn with_opcode<Op: NetOpcode>(self, opcode: Op) -> ResponsePacket<Self> {
        ResponsePacket::new(opcode, self)
    }
}

impl<T: EncodePacket> PacketOpcodeExt for T {}

///  Provides conversion from types into actual Responses
pub trait IntoResponse {
    type Resp: Response + Send;

    /// Converts this type into an actual response
    fn into_response(self) -> Self::Resp;
}

impl IntoResponse for () {
    type Resp = ();

    fn into_response(self) -> Self::Resp {}
}

impl<T: IntoResponse> IntoResponse for Option<T> {
    type Resp = Option<T::Resp>;

    fn into_response(self) -> Self::Resp {
        self.map(|r| r.into_response())
    }
}

/* TODO
impl<T: EncodePacket + HasOpcode + Send> IntoResponse for T {
    type Resp = ResponsePacket<T>;

    fn into_response(self) -> Self::Resp {
        ResponsePacket::new(T::OPCODE, self)
    }
}*/

impl<T: EncodePacket + HasOpcode + Send> IntoResponse for Vec<T> {
    type Resp = Vec<ResponsePacket<T>>;

    fn into_response(self) -> Self::Resp {
        self.into_iter()
            .map(|p| ResponsePacket::new(T::OPCODE, p))
            .collect()
    }
}

impl<T: EncodePacket + Send> IntoResponse for ResponsePacket<T> {
    type Resp = ResponsePacket<T>;

    fn into_response(self) -> Self::Resp {
        self
    }
}

impl<T: EncodePacket + Send> IntoResponse for Vec<ResponsePacket<T>> {
    type Resp = Vec<ResponsePacket<T>>;

    fn into_response(self) -> Self::Resp {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{IntoResponse, ResponsePacket};

    fn check_is_into_response<T>() -> bool
    where
        T: IntoResponse,
    {
        true
    }

    #[test]
    fn name() {
        check_is_into_response::<()>();
        check_is_into_response::<Option<()>>();
        check_is_into_response::<ResponsePacket<()>>();
        check_is_into_response::<Vec<ResponsePacket<()>>>();
    }
}
