use std::{
    str::FromStr as _,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result};
use base64::{Engine, prelude::BASE64_STANDARD};
use bytes::Bytes;
use futures::{SinkExt, StreamExt as _};
use iroh::{
    Endpoint, NodeAddr, RelayMap, RelayMode, RelayUrl, SecretKey,
    endpoint::{Builder, ConnectOptions, TransportConfig},
    watchable::Watcher,
};
use iroh_quinn_proto::congestion::{BbrConfig, CubicConfig};
use raptorq::{Decoder, Encoder, EncodingPacket, ObjectTransmissionInformation};
use tokio::io::AsyncWriteExt as _;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() == 3 {
        if args[1] != "--connect" {
            println!("Usage: iroh-p2p-node --connect <address>");
            return Ok(());
        }
        // let pkey = PublicKey::from_z32(&args[2])?;
        // let addr = EndpointAddr::new(pkey);
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
            // RelayUrl::from_str("https://aps1-1.relay.n0.iroh-canary.iroh.link.").unwrap(),
            // RelayUrl::from_str("https://daibo.ermis.network.").unwrap(),
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
    let mut cur_addr = ep.node_addr().await.unwrap();
    while let Some(incoming) = ep.accept().await {
        // let mut watcher = ep.watch_addr();
        let c_a = cur_addr.clone();
        tokio::spawn(async move {
            let conn = incoming.accept().unwrap().await.unwrap();
            let stream = conn.accept_uni().await.unwrap();
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
            // loop {
            //     let dgram = conn.read_datagram().await.unwrap();
            //     if dgram.len() == 12 {
            //         let transmission_info =
            //             ObjectTransmissionInformation::deserialize(dgram[..12].try_into().unwrap());
            //         let mut decoder = Decoder::new(transmission_info);
            //         let mut received = 0;
            //         loop {
            //             let dgram = conn.read_datagram().await?;
            //             received += 1;
            //             let packet = EncodingPacket::deserialize(&dgram);
            //             if packet.payload_id().encoding_symbol_id() == 39 {
            //                 println!("{}", packet.data().len());
            //                 continue;
            //             }
            //             // println!("received: {}", packet.payload_id().encoding_symbol_id());
            //             decoder.add_new_packet(packet);
            //             if let Some(decoded) = decoder.get_result() {
            //                 println!(
            //                     "{}, {}",
            //                     u32::from_be_bytes(decoded[0..4].try_into().unwrap()),
            //                     decoded.len()
            //                 );
            //                 break;
            //             }
            //         }
            //     }
            // }
            // Ok(())
        });
    }
    Ok(())
}

async fn connect(addr: NodeAddr) -> Result<()> {
    let ep = Endpoint::builder()
        .relay_mode(RelayMode::Custom(RelayMap::from_iter(vec![
            RelayUrl::from_str("https://test-iroh.ermis.network.:8443").unwrap(),
            // RelayUrl::from_str("https://aps1-1.relay.n0.iroh-canary.iroh.link.").unwrap(),
            // RelayUrl::from_str("https://daibo.ermis.network.").unwrap(),
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
    let stream = conn.open_uni().await?;
    let mut framed_write = FramedWrite::new(stream, LengthDelimitedCodec::new());
    let mut seq = 0u32;
    let mut msg = [0u8; 10004];
    let mut lost_packets = 0;
    loop {
        msg[..4].copy_from_slice(&seq.to_be_bytes());
        framed_write.send(Bytes::copy_from_slice(&msg)).await?;
        // let encoder = Encoder::with_defaults(&msg, 500);
        // conn.send_datagram(Bytes::copy_from_slice(
        //     encoder.get_config().serialize().as_slice(),
        // ))
        // .unwrap();
        // lost_packets = conn.stats().path.lost_packets;
        // for packet in encoder
        //     .get_encoded_packets(1)
        //     .iter()
        //     .map(|packet| packet.serialize())
        // {
        //     println!("{}", conn.datagram_send_buffer_space());
        //     conn.send_datagram(Bytes::from(packet));
        //     tokio::time::sleep(std::time::Duration::from_micros(1000)).await;
        // }

        let cur_lost = conn.stats().path.lost_packets - lost_packets;
        let c_t = conn_type.get().unwrap();
        println!(
            "{}, {}ms, {}, {}",
            c_t,
            conn.rtt().as_millis(),
            cur_lost,
            conn.max_datagram_size().unwrap()
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        seq += 1;
    }
}
