use shroom_pkt::shroom_enum_code;
use tokio::{io::AsyncWriteExt, net::TcpStream};

use crate::{crypto::SharedCryptoContext, NetResult};

use self::{
    codec::{LegacyDecoder, LegacyEncoder},
    handshake_gen::{BasicHandshakeGenerator, HandshakeGenerator},
};

use super::ShroomCodec;

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

pub struct LegacyCodec {
    crypto_ctx: SharedCryptoContext,
    handshake_gen: BasicHandshakeGenerator,
}

impl LegacyCodec {
    pub fn new(crypto_ctx: SharedCryptoContext, handshake_gen: BasicHandshakeGenerator) -> Self {
        Self {
            crypto_ctx,
            handshake_gen,
        }
    }
    /*
    pub fn create_client(ctx: SharedCryptoContext, handshake: Handshake) -> Self {
        let v = ShroomVersion(handshake.version);
        Self {
            decode: LegacyDecoder(ShroomCrypto::new(ctx.clone(), handshake.iv_dec, v.invert())),
            encode: LegacyEncoder(ShroomCrypto::new(ctx, handshake.iv_enc, v)),
        }
    }

    pub fn from_server_handshake(ctx: SharedCryptoContext, handshake: Handshake) -> Self {
        let v = ShroomVersion(handshake.version);
        Self {
            decode: LegacyDecoder(ShroomCrypto::new(ctx.clone(), handshake.iv_enc, v)),
            encode: LegacyEncoder(ShroomCrypto::new(ctx, handshake.iv_dec, v.invert())),
        }
    }*/
}

#[async_trait::async_trait]
impl ShroomCodec for LegacyCodec {
    type Encoder = LegacyEncoder;
    type Decoder = LegacyDecoder;
    type Transport = TcpStream;

    async fn create_client_session(
        &self,
        mut trans: Self::Transport,
    ) -> NetResult<(Self::Encoder, Self::Decoder)> {
        let handshake = self.handshake_gen.generate_handshake();
        let buf = handshake.to_buf();
        trans.write_all(&buf).await?;

        todo!()
    }
    async fn create_server_session(
        &self,
        trans: Self::Transport,
    ) -> NetResult<(Self::Encoder, Self::Decoder)> {
        todo!()
    }
}
