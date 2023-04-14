use async_trait::async_trait;

use crate::{
    net::{SessionTransport, ShroomSession},
    EncodePacket, HasOpcode, NetOpcode, NetResult,
};

use super::handler::SessionHandleResult;

//TODO: either remove async_trait for performance reasons
// or wait for async fn's in trait becoming stable

/// Represents a response which can be sent with the session
/// Returning a SessionHandleResult
#[async_trait]
pub trait Response {
    async fn send<Trans: SessionTransport + Send + Unpin>(
        self,
        session: &mut ShroomSession<Trans>,
    ) -> NetResult<SessionHandleResult>;
}

/// Unit is essentially a No-Op
#[async_trait]
impl Response for () {
    async fn send<Trans: SessionTransport + Send + Unpin>(
        self,
        _session: &mut ShroomSession<Trans>,
    ) -> NetResult<SessionHandleResult> {
        Ok(SessionHandleResult::Ok)
    }
}

/// Sending the value Some value If It's set
#[async_trait]
impl<Resp: Response + Send> Response for Option<Resp> {
    async fn send<Trans: SessionTransport + Send + Unpin>(
        self,
        session: &mut ShroomSession<Trans>,
    ) -> NetResult<SessionHandleResult> {
        match self {
            Some(resp) => resp.send(session).await,
            None => Ok(SessionHandleResult::Ok),
        }
    }
}

/// Sending all Responses in this `Vec`
#[async_trait]
impl<Resp: Response + Send> Response for Vec<Resp> {
    async fn send<Trans: SessionTransport + Send + Unpin>(
        self,
        session: &mut ShroomSession<Trans>,
    ) -> NetResult<SessionHandleResult> {
        for resp in self.into_iter() {
            resp.send(session).await?;
        }
        Ok(SessionHandleResult::Ok)
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
    async fn send<Trans: SessionTransport + Send + Unpin>(
        self,
        session: &mut ShroomSession<Trans>,
    ) -> NetResult<SessionHandleResult> {
        session.send_packet_with_opcode(self.op, self.data).await?;
        Ok(SessionHandleResult::Ok)
    }
}

/// Response which sends the packet `T` and then
/// migrates the session
pub struct MigrateResponse<T>(pub T);

#[async_trait]
impl<T> Response for MigrateResponse<T>
where
    T: Response + Send,
{
    async fn send<Trans: SessionTransport + Send + Unpin>(
        self,
        session: &mut ShroomSession<Trans>,
    ) -> NetResult<SessionHandleResult> {
        self.0.send(session).await?;
        return Ok(SessionHandleResult::Migrate);
    }
}

/// Response which does nothing but signals that a Pong was handled via `SessionHandleResult`
pub struct PongResponse;

#[async_trait]
impl Response for PongResponse {
    async fn send<Trans: SessionTransport + Send + Unpin>(
        self,
        _session: &mut ShroomSession<Trans>,
    ) -> NetResult<SessionHandleResult> {
        return Ok(SessionHandleResult::Pong);
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

    fn into_response(self) -> Self::Resp {
        ()
    }
}

impl<T: IntoResponse> IntoResponse for Option<T> {
    type Resp = Option<T::Resp>;

    fn into_response(self) -> Self::Resp {
        self.map(|r| r.into_response())
    }
}

impl<T: EncodePacket + HasOpcode + Send> IntoResponse for T {
    type Resp  = ResponsePacket<T>;

    fn into_response(self) -> Self::Resp {
        ResponsePacket::new(T::OPCODE, self)
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
