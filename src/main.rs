use std::{str::FromStr as _, sync::Arc, time::Instant};

use anyhow::{Context as _, Ok, Result};
use base64::{Engine, prelude::BASE64_STANDARD};
use bytes::Bytes;
use iroh::{
    EndpointAddr, RelayMap, RelayMode, RelayUrl, Watcher,
    endpoint::{Builder, ConnectOptions, TransportConfig},
};
use iroh_quinn_proto::congestion::{BbrConfig, CubicConfig};
use raptorq::{Decoder, Encoder, EncodingPacket, ObjectTransmissionInformation};
use tokio::io::AsyncWriteExt as _;
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
        let addr: EndpointAddr = bitcode::deserialize(&addr_bytes)?;
        println!("connecting to peer {:?}", addr);
        return connect(addr).await;
    }
    let ep = Builder::empty(RelayMode::Custom(RelayMap::from_iter(vec![
        RelayUrl::from_str("https://test-iroh.ermis.network.:8443").unwrap(),
        // RelayUrl::from_str("https://aps1-1.relay.n0.iroh-canary.iroh.link.").unwrap(),
        // RelayUrl::from_str("https://daibo.ermis.network.").unwrap(),
    ])))
    .alpns(vec![b"ermis-call".to_vec()])
    .bind()
    .await?;
    ep.online().await;
    let addr_bytes = bitcode::serialize(&ep.addr()).unwrap();
    let addr_str = base64::prelude::BASE64_STANDARD.encode(addr_bytes);
    println!("{:?}", ep.addr());
    // println!("{}", ep.id().to_z32());
    println!("{}", addr_str);
    let mut cur_addr = ep.addr();
    while let Some(incoming) = ep.accept().await {
        let mut watcher = ep.watch_addr();
        let c_a = cur_addr.clone();
        tokio::spawn(async move {
            let (conn) = incoming.accept()?.await?;
            // let conn = incoming.await.context("connecting error")?;
            println!("{:?}", conn.remote_id());
            let addr = watcher.get();
            if addr != c_a {
                // ep.network_change().await;
                println!("network changed: {:?}", addr);
            }
            let (mut send_stream, mut recv_stream) = conn.accept_bi().await.unwrap();
            println!("bidi accepted");
            let mut buf = [0u8; 80000];
            loop {
                recv_stream.read_exact(&mut buf[0..4]).await?;
                let len = u32::from_be_bytes(buf[0..4].try_into().unwrap());
                let n = recv_stream
                    .read_exact(&mut buf[4..len as usize + 4])
                    .await
                    .context("unable to read")?;
                println!("{:?}, {:?}", len, Instant::now());
            }
            // loop {
            //     let dgram = conn.read_datagram().await?;
            //     if dgram.len() == 12 {
            //         let transmission_info =
            //             ObjectTransmissionInformation::deserialize(dgram[..12].try_into().unwrap());
            //         let mut decoder = Decoder::new(transmission_info);
            //         loop {
            //             let dgram = conn.read_datagram().await?;
            //             let packet = EncodingPacket::deserialize(&dgram);
            //             let decoded = decoder.decode(packet);
            //             if let Some(decoded) = decoded {
            //                 println!(
            //                     "{}, {}",
            //                     u32::from_be_bytes(decoded[0..4].try_into().unwrap()),
            //                     decoded.len()
            //                 );
            //                 break;
            //             }
            //         }
            //     }
            //     // println!(
            //     //     "{}, {}",
            //     //     u32::from_be_bytes(dgram[0..4].try_into().unwrap()),
            //     //     dgram.len()
            //     // )
            // }
            Ok(())
        });
    }
    Ok(())
}

async fn connect(addr: EndpointAddr) -> Result<()> {
    let ep = Builder::empty(RelayMode::Custom(RelayMap::from_iter(vec![
        RelayUrl::from_str("https://test-iroh.ermis.network.:8443").unwrap(),
        // RelayUrl::from_str("https://aps1-1.relay.n0.iroh-canary.iroh.link.").unwrap(),
        // RelayUrl::from_str("https://daibo.ermis.network.").unwrap(),
    ])))
    .alpns(vec![b"my-alpn".to_vec()])
    .bind()
    .await?;
    ep.online().await;
    println!("{:?}", ep.addr());
    let mut transport_config = TransportConfig::default();
    transport_config.congestion_controller_factory(std::sync::Arc::new(CubicConfig::default()));
    let conn = ep
        .connect_with_opts(
            addr.clone(),
            b"ermis-call",
            ConnectOptions::new().with_transport_config(Arc::new(transport_config)),
        )
        .await?
        .await?;
    let mut conn_type = ep.conn_type(addr.id).unwrap();
    let (mut send_stream, mut recv_stream) = conn.open_bi().await.context("unable to open uni")?;
    let mut seq = 0u32;
    let mut msg = [0u8; 10004];
    let mut cur_lost = 0;
    loop {
        msg[..4].copy_from_slice(&10000u32.to_be_bytes());
        for chunk in msg.chunks(500) {
            if conn.stats().path.lost_packets > cur_lost {
                println!("Lost packets: {}", conn.stats().path.lost_packets);
                cur_lost = conn.stats().path.lost_packets;
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            send_stream
                .write_all(chunk)
                .await
                .context("unable to write all")
                .unwrap();
            send_stream.flush().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }

        // let encoder = Encoder::with_defaults(&msg, conn.max_datagram_size().unwrap() as u16 - 100);
        // conn.send_datagram(Bytes::copy_from_slice(
        //     encoder.get_config().serialize().as_slice(),
        // ))
        // .unwrap();
        // for packet in encoder
        //     .get_encoded_packets(1)
        //     .iter()
        //     .map(|packet| packet.serialize())
        // {
        //     conn.send_datagram(Bytes::from(packet)).unwrap();
        //     tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        // }

        // conn.send_datagram(Bytes::copy_from_slice(&msg)).unwrap();
        // tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let c_t = conn_type.get();
        println!(
            "{}, {}ms, {}, {}",
            c_t,
            conn.rtt().as_millis(),
            conn.stats().path.lost_packets as f64 / conn.stats().path.sent_packets as f64,
            conn.max_datagram_size().unwrap()
        );
        seq += 1;
    }
}
