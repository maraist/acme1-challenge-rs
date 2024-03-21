use axum::body::Body;
use axum::Json;
use axum_macros::debug_handler;
use futures::TryStreamExt;
use serde::Serialize;
use tokio::fs::File;
use tokio::io::BufWriter;
use tokio_util::io::StreamReader;

const X25: crc::Crc<u16> = crc::Crc::<u16>::new(&crc::CRC_16_IBM_SDLC);

#[derive(Debug, Serialize)]
pub struct CrcJsonResponse {
    crc: u16,
}
#[debug_handler]
pub async fn handle_crc(body: Body) -> Json<CrcJsonResponse> {
    let stream = body
        .into_data_stream()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));
    async {
        let sr = StreamReader::new(stream);
        futures::pin_mut!(sr);
        let mut file = BufWriter::new(File::create("/tmp/ul.dat").await.unwrap());
        let sz = tokio::io::copy(&mut sr, &mut file).await.unwrap();
        println!("crc src read {sz} bytes");
    }
    .await;
    let buf = vec![0u8; 512];
    let chk = X25.checksum(&buf);
    Json(CrcJsonResponse { crc: chk })
}
