use tokio::time::{sleep, Duration};
use mysql::*;
use mysql::prelude::*;
use reqwest;
use serde::{Serialize};
use serde_json::Value;
use warp::Filter;
use actix_cors::Cors;  // 引入 actix_cors
use tokio::sync::broadcast;
use anyhow::Result;
use futures_util::{StreamExt, SinkExt};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Serialize)]
struct BlockData {
    peer_count: i32,
    medium_fee_per_kb: f64,
    price: f64,
    time: String,
}

async fn fetch_blockchain_data() -> Result<(i32, f64)> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;

    let response = client.get("https://api.blockcypher.com/v1/btc/main").send().await?;
    let json: Value = response.json().await?;

    println!("Received JSON: {:?}", json);  // 日志打印，确认接收到的数据

    let peer_count = json["peer_count"].as_i64().ok_or(anyhow::anyhow!("Peer count field missing or invalid"))? as i32;
    let medium_fee_per_kb = json["medium_fee_per_kb"].as_f64().ok_or(anyhow::anyhow!("Medium fee per KB field missing or invalid"))?;

    Ok((peer_count, medium_fee_per_kb))
}

async fn fetch_bitcoin_price() -> Result<f64> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;
    
    let response = client
        .get("https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies=usd")
        .send()
        .await?;
    let json: Value = response.json().await?;
    
    println!("Received Bitcoin price from API: {:?}", json);  // 日志打印，确认接收到比特币价格

    Ok(json["bitcoin"]["usd"].as_f64().unwrap())
}

async fn user_connected(ws: warp::ws::WebSocket, mut rx: tokio::sync::broadcast::Receiver<String>) {
    let (mut ws_tx, _rx) = ws.split();
    println!("A user has connected.");

    while let Ok(msg) = rx.recv().await {
        if ws_tx.send(warp::ws::Message::text(msg)).await.is_err() {
            break;
        }
    }
    println!("A user has disconnected.");
}

async fn get_latest_blocks(mut conn: PooledConn) -> Result<Vec<BlockData>> {
    let result: Vec<(i32, f64, f64)> = conn
        .query("SELECT peer_count, medium_fee_per_kb, price FROM blocks ORDER BY id DESC LIMIT 10")
        .unwrap();

    let data: Vec<BlockData> = result
        .into_iter()
        .map(|(peer_count, medium_fee_per_kb, price)| BlockData {
            peer_count,
            medium_fee_per_kb,
            price,
            time: chrono::Utc::now().to_rfc3339(),
        })
        .collect();

    Ok(data)
}

#[tokio::main]
async fn main() {
    let tx = Arc::new(Mutex::new(broadcast::channel(100).0));

    // 设置 CORS 配置，允许来自所有来源的请求
    let cors = warp::cors()
        .allow_any_origin()  // 允许所有来源的请求
        .allow_methods(vec!["GET", "POST", "DELETE"])  // 允许的 HTTP 方法
        .allow_headers(vec!["Content-Type"]);  // 允许的请求头

    let websocket_route = warp::path("ws")
        .and(warp::ws())
        .map({
            let tx = Arc::clone(&tx);
            move |ws: warp::ws::Ws| {
                let tx = Arc::clone(&tx);
                ws.on_upgrade(move |socket| {
                    let tx = Arc::clone(&tx);
                    async move {
                        let rx = {
                            let tx = tx.lock().await;
                            tx.subscribe()
                        };
                        user_connected(socket, rx).await;
                    }
                })
            }
        });

    let url = "mysql://caitlynn:123456@34.44.123.189:3308/help_bitcoin";
    let pool = loop {
        match Pool::new(url) {
            Ok(pool) => break pool,
            Err(_) => {
                println!("Failed to connect to MySQL, retrying...");
                sleep(Duration::from_secs(5)).await;
            }
        };
    };

    let latest_blocks_route = warp::path("latest_blocks")
        .and(warp::get())
        .and_then({
            let pool = pool.clone();
            move || {
                let conn = pool.get_conn().unwrap();
                async move {
                    let blocks = get_latest_blocks(conn).await.unwrap();
                    Ok::<_, warp::Rejection>(warp::reply::json(&blocks))
                }
            }
        });

    // 添加 CORS 支持到所有路由
    let routes = websocket_route
        .or(latest_blocks_route)
        .with(cors);

    let mut conn = pool.get_conn().unwrap();
    let tx_clone = Arc::clone(&tx);

    tokio::spawn(async move {
        loop {
            println!("Fetching blockchain data...");  // 添加日志，确认定时任务是否在运行

            match fetch_blockchain_data().await {
                Ok((peer_count, medium_fee_per_kb)) => {
                    println!("Fetched data: Peer count: {}, Medium fee per KB: {}", peer_count, medium_fee_per_kb);  // 添加日志，确认数据是否正确

                    conn.exec_drop(
                        "INSERT INTO blocks (peer_count, medium_fee_per_kb, price) VALUES (:peer_count, :medium_fee_per_kb, :price)",
                        params! {
                            "peer_count" => peer_count,
                            "medium_fee_per_kb" => medium_fee_per_kb,
                            "price" => fetch_bitcoin_price().await.unwrap(),
                        }
                    ).unwrap();
    
                    let price = fetch_bitcoin_price().await.unwrap();
                    let message = format!(
                        r#"{{"peer_count": {}, "medium_fee_per_kb": {}, "price": {}}}"#,
                        peer_count, medium_fee_per_kb, price
                    );
    
                    if let Err(e) = {
                        let tx = tx_clone.lock().await;
                        tx.send(message.clone())
                    } {
                        println!("Failed to broadcast message: {}", e);
                    } else {
                        println!("Broadcasted message: {}", message);
                    }
                    println!("Inserted: Peer Count: {}, Medium Fee per KB: {}, Price: {}", peer_count, medium_fee_per_kb, price);  // 添加日志，确认插入操作成功
                }
                Err(e) => {
                    println!("Failed to fetch blockchain data: {}", e);
                }
            }
    
            sleep(Duration::from_secs(60)).await;
        }
    });

    println!("WebSocket server started at ws://0.0.0.0:3030/ws");

    warp::serve(routes).run(([0, 0, 0, 0], 3030)).await;
}
