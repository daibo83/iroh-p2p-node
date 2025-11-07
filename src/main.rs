use std::{str::FromStr as _, sync::Arc};

use anyhow::{Context as _, Ok, Result};
use base64::{Engine, prelude::BASE64_STANDARD};
use iroh::{
    EndpointAddr, RelayMap, RelayMode, RelayUrl, Watcher,
    endpoint::{Builder, ConnectOptions, TransportConfig},
};
use iroh_quinn_proto::congestion::BbrConfig;
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
    .alpns(vec![b"my-alpn".to_vec()])
    .bind()
    .await?;
    ep.online().await;
    let addr_bytes = bitcode::serialize(&ep.addr()).unwrap();
    let addr_str = base64::prelude::BASE64_STANDARD.encode(addr_bytes);
    println!("{:?}", ep.addr());
    // println!("{}", ep.id().to_z32());
    println!("{}", addr_str);
    while let Some(incoming) = ep.accept().await {
        tokio::spawn(async move {
            let (conn, za) = incoming.accept()?.into_0rtt().unwrap();
            // let conn = incoming.await.context("connecting error")?;
            print!("{:?}", conn.remote_id());
            let mut recv_stream = conn.accept_uni().await.unwrap();
            loop {
                let mut buf = [0u8; 8];
                recv_stream
                    .read_exact(&mut buf)
                    .await
                    .context("unable to read")?;
                println!("{:?}", u64::from_be_bytes(buf));
            }
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
    transport_config.congestion_controller_factory(std::sync::Arc::new(BbrConfig::default()));
    let conn = ep
        .connect_with_opts(
            addr.clone(),
            b"my-alpn",
            ConnectOptions::new().with_transport_config(Arc::new(transport_config)),
        )
        .await?
        .await?;
    let mut conn_type = ep.conn_type(addr.id).unwrap();
    let mut send_stream = conn.open_uni().await.context("unable to open uni")?;
    let mut seq = 0u64;
    loop {
        send_stream
            .write_all(&seq.to_be_bytes())
            .await
            .context("unable to write all")?;
        seq += 1;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let c_t = conn_type.get();
        println!("{}, {}ms", c_t, conn.rtt().as_millis());
    }
}
