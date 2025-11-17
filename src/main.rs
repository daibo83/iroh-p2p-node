use std::{
    str::FromStr as _,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Result;
use base64::{Engine, prelude::BASE64_STANDARD};
use bytes::Bytes;
use futures::{SinkExt, StreamExt as _};
use iroh::{
    Endpoint, NodeAddr, RelayMap, RelayMode, RelayUrl,
    endpoint::{ConnectOptions, TransportConfig},
};
use iroh_quinn_proto::congestion::{BbrConfig, CubicConfig};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() == 3 {
        if args[1] != "--connect" {
            println!("Usage: iroh-p2p-node --connect <address>");
            return Ok(());
        }
        let addr_bytes = BASE64_STANDARD.decode(&args[2])?;
        let addr: NodeAddr = bitcode::deserialize(&addr_bytes)?;
        println!("connecting to peer {:?}", addr);
        return connect(addr).await;
    }
    let mut transport_config = TransportConfig::default();
    transport_config.congestion_controller_factory(std::sync::Arc::new(CubicConfig::default()));
    transport_config.keep_alive_interval(Some(Duration::from_secs(5)));
    let ep = Endpoint::builder()
        .relay_mode(RelayMode::Custom(RelayMap::from_iter(vec![
            RelayUrl::from_str("https://test-iroh.ermis.network.:8443").unwrap(),
        ])))
        .transport_config(transport_config)
        .alpns(vec![b"ermis-call".to_vec()])
        .bind()
        .await?;
    // ep.online().await;
    let addr_bytes = bitcode::serialize(&ep.node_addr().await.unwrap()).unwrap();
    let addr_str = base64::prelude::BASE64_STANDARD.encode(addr_bytes);
    println!("{:?}", ep.node_addr().await.unwrap());
    // println!("{}", ep.id().to_z32());
    println!("{}", addr_str);
    while let Some(incoming) = ep.accept().await {
        // let mut watcher = ep.watch_addr();

        tokio::spawn(async move {
            let conn = incoming.accept().unwrap().await.unwrap();
            let (_, stream) = conn.accept_bi().await.unwrap();
            let mut framed_read = FramedRead::new(stream, LengthDelimitedCodec::new());
            while let Some(frame) = framed_read.next().await {
                match frame {
                    Ok(frame) => {
                        println!(
                            "{}, {}",
                            u32::from_be_bytes(frame[0..4].try_into().unwrap()),
                            frame.len()
                        )
                    }
                    Err(err) => {
                        println!("{}", err);
                        break;
                    }
                }
            }
        });
    }
    Ok(())
}

async fn connect(addr: NodeAddr) -> Result<()> {
    let ep = Endpoint::builder()
        .relay_mode(RelayMode::Custom(RelayMap::from_iter(vec![
            RelayUrl::from_str("https://test-iroh.ermis.network.:8443").unwrap(),
        ])))
        .alpns(vec![b"my-alpn".to_vec()])
        .bind()
        .await?;
    // ep.online().await;
    println!("{:?}", ep.node_addr().await.unwrap());
    let mut transport_config = TransportConfig::default();
    transport_config.congestion_controller_factory(std::sync::Arc::new(BbrConfig::default()));
    transport_config.keep_alive_interval(Some(Duration::from_secs(5)));
    let conn = ep
        .connect_with_opts(
            addr.clone(),
            b"ermis-call",
            ConnectOptions::new().with_transport_config(Arc::new(transport_config)),
        )
        .await?
        .await?;
    let conn_type = ep.conn_type(addr.node_id).unwrap();
    let (stream, _) = conn.open_bi().await?;
    let mut framed_write = FramedWrite::new(stream, LengthDelimitedCodec::new());
    let mut seq = 0u32;
    let mut msg = [0u8; 10004];
    let mut lost_packets = 0;
    let mut now = Instant::now();
    let mut cur_direct_addresses = ep.direct_addresses().get().unwrap().unwrap();
    loop {
        msg[..4].copy_from_slice(&seq.to_be_bytes());
        framed_write.send(Bytes::copy_from_slice(&msg)).await?;
        let cur_lost = conn.stats().path.lost_packets - lost_packets;
        lost_packets = conn.stats().path.lost_packets;
        let c_t = conn_type.get().unwrap();
        let direct_addresses = ep.direct_addresses().get().unwrap().unwrap();
        if cur_direct_addresses != direct_addresses {
            cur_direct_addresses = direct_addresses;
            println!("Direct addresses changed: {:?}", cur_direct_addresses);
        }
        if now.elapsed().as_millis() >= 1000 {
            println!(
                "{}, {}ms, {}, {}",
                c_t,
                conn.rtt().as_millis(),
                cur_lost,
                conn.max_datagram_size().unwrap(),
            );
            now = Instant::now();
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        seq += 1;
    }
}
