use shroom_pkt::shroom_enum_code;
use tokio::io::AsyncWriteExt;

use crate::{
    crypto::{SharedCryptoContext, ShroomCrypto, ShroomVersion},
    NetResult,
};

use self::{
    codec::{LegacyDecoder, LegacyEncoder},
    handshake::Handshake,
    handshake_gen::{BasicHandshakeGenerator, HandshakeGenerator},
};

use super::{conn::ShroomConn, ShroomCodec, ShroomTransport};

pub mod codec;
pub mod handshake;
pub mod handshake_gen;

pub const MAX_HANDSHAKE_LEN: usize = 24;
pub const MAX_PACKET_LEN: usize = i16::MAX as usize;

// Locale code for handshake, T means test server
shroom_enum_code!(
    LocaleCode,
    u8,
    Korea = 1,
    KoreaT = 2,
    Japan = 3,
    China = 4,
    ChinaT = 5,
    Taiwan = 6,
    TaiwanT = 7,
    Global = 8,
    Europe = 9,
    RlsPe = 10
);

pub struct LegacyCodec<T = tokio::net::TcpStream> {
    crypto_ctx: SharedCryptoContext,
    handshake_gen: BasicHandshakeGenerator,
    _marker: std::marker::PhantomData<T>,
}

impl<T> Default for LegacyCodec<T> {
    fn default() -> Self {
        Self::new(
            SharedCryptoContext::default(),
            BasicHandshakeGenerator::v95(),
        )
    }
}

impl<T> LegacyCodec<T> {
    pub fn new(crypto_ctx: SharedCryptoContext, handshake_gen: BasicHandshakeGenerator) -> Self {
        Self {
            crypto_ctx,
            handshake_gen,
            _marker: std::marker::PhantomData,
        }
    }

    #[cfg(test)]
    pub(crate) fn create_mock_client_codec(&self) -> (LegacyEncoder, LegacyDecoder) {
        let hshake = self.handshake_gen.generate_handshake();
        self.create_client_codec(&hshake)
    }

    fn create_client_codec(&self, handshake: &Handshake) -> (LegacyEncoder, LegacyDecoder) {
        let v = ShroomVersion(handshake.version);
        (
            LegacyEncoder(ShroomCrypto::new(
                self.crypto_ctx.clone(),
                handshake.iv_enc,
                v,
            )),
            LegacyDecoder(ShroomCrypto::new(
                self.crypto_ctx.clone(),
                handshake.iv_dec,
                v.invert(),
            )),
        )
    }

    fn create_server_codec(&self, handshake: &Handshake) -> (LegacyEncoder, LegacyDecoder) {
        let v = ShroomVersion(handshake.version);
        (
            LegacyEncoder(ShroomCrypto::new(
                self.crypto_ctx.clone(),
                handshake.iv_dec,
                v.invert(),
            )),
            LegacyDecoder(ShroomCrypto::new(
                self.crypto_ctx.clone(),
                handshake.iv_enc,
                v,
            )),
        )
    }
}

#[async_trait::async_trait]
impl<T: ShroomTransport + Sync> ShroomCodec for LegacyCodec<T> {
    type Encoder = LegacyEncoder;
    type Decoder = LegacyDecoder;
    type Transport = T;

    async fn create_client_session(
        &self,
        mut trans: Self::Transport,
    ) -> NetResult<ShroomConn<Self>> {
        // Read handshake from the server
        let hshake = Handshake::read_handshake_async(&mut trans).await?;
        Ok(ShroomConn::new(trans, self.create_client_codec(&hshake)))
    }
    async fn create_server_session(
        &self,
        mut trans: Self::Transport,
    ) -> NetResult<ShroomConn<Self>> {
        let hshake = self.handshake_gen.generate_handshake();
        trans.write_all(&hshake.to_buf()).await?;
        Ok(ShroomConn::new(trans, self.create_server_codec(&hshake)))
    }
}
