use futures_util::StreamExt;
use futures_util::sink::SinkExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::protocol::Message;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
    RTCIceCandidateInit, RTCIceGatheringState, RTCIceServer, RTCPeerConnectionIceEvent,
    RTCSessionDescription,
};

const SERVER: &str = "wss://p2p-chess-signaling-server.onrender.com/ws";
// const SERVER: &str = "ws://127.0.0.1:3000/ws";

#[derive(Debug, Serialize, Deserialize)]
enum Msg {
    GatherComplete,
    IceCandidate(RTCIceCandidateInit),
    Offer(RTCSessionDescription),
    Answer(RTCSessionDescription),
    UciMove(String),
    NewGame(String),
    Initiator,
}

// 1. Implement the PeerConnectionEventHandler trait to handle events
#[derive(Clone)]
struct EventHandler {
    tx: mpsc::Sender<Msg>,
    dc_tx: mpsc::Sender<mpsc::Sender<Msg>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for EventHandler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        if let Ok(candidate) = event.candidate.to_json() {
            self.tx.send(Msg::IceCandidate(candidate)).await.unwrap();
            println!("New local ICE candidate gathered: {}", event.candidate);
        }
    }
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            dbg!("gathering complete");
            self.tx.send(Msg::GatherComplete).await.unwrap();
        }
    }
    async fn on_data_channel(&self, channel: Arc<dyn DataChannel>) {
        let (tx, mut rx) = mpsc::channel(100);
        self.dc_tx.send(tx).await.unwrap();

        let tx = self.tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(event) = channel.poll() => {
                        match event {
                            DataChannelEvent::OnOpen => {
                                println!("Channel Open!");
                            }
                            DataChannelEvent::OnMessage(msg) => {
                                let text = String::from_utf8_lossy(&msg.data);
                                let peer_msg: Msg = serde_json::from_str(&text).unwrap();
                                tx.send(peer_msg).await.unwrap();
                            }
                            DataChannelEvent::OnClose => {
                                println!("channel closed");
                                break;
                            }
                            _ => {}
                        }
                    }
                    Some(input) = rx.recv() => {
                        let json = serde_json::to_string(&input).unwrap();
                        channel.send_text(&json).await.unwrap();
                    }
                }
            }
        });
    }
}

pub fn connect() -> impl iced::futures::Stream<Item = crate::Message> {
    iced::stream::channel(100, async |mut output| {
        let (app_tx, mut app_rx) = mpsc::channel(100);
        output
            .send(crate::Message::Connected(app_tx))
            .await
            .unwrap();

        // 2. Configure the peer connection
        let config = RTCConfigurationBuilder::default()
            .with_ice_servers(vec![RTCIceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_owned()],
                ..Default::default()
            }])
            .build();

        let (tx, mut rx) = mpsc::channel(100);
        let (dc_tx, mut dc_rx) = mpsc::channel(100);

        let event_handler = EventHandler { tx, dc_tx };

        // 3. Build the PeerConnection
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_handler(Arc::new(event_handler))
            .with_tcp_addrs(vec!["0.0.0.0:0"])
            .build()
            .await
            .unwrap();

        let mut socket = match tokio_tungstenite::connect_async(SERVER).await {
            Ok((stream, _response)) => {
                println!("WebSocket handshake has been completed");
                stream
            }
            Err(e) => {
                println!("WebSocket handshake failed with {e}!");
                return;
            }
        };

        // set initiator
        let mut initiator = false;
        while let Some(msg) = socket.next().await {
            match msg {
                Ok(m) => match m {
                    Message::Text(s) => {
                        if s.as_str() == "true" {
                            initiator = true;
                            break;
                        }
                        if s.as_str() == "false" {
                            break;
                        }
                    }
                    Message::Close(_) => return,
                    _ => {}
                },
                Err(_) => return,
            }
        }
        dbg!(initiator);

        // if initiator, send offer
        if initiator {
            pc.create_data_channel("main", None).await.unwrap();

            // Create an SDP offer and set it as local description
            let offer = pc.create_offer(None).await.unwrap();
            pc.set_local_description(offer.clone()).await.unwrap();

            while let Some(msg) = rx.recv().await {
                match msg {
                    Msg::GatherComplete => break,
                    _ => {}
                }
            }

            // send the offer
            let offer_msg = Msg::Offer(offer);
            let json = serde_json::to_string(&offer_msg).unwrap();
            socket.send(Message::Text(json.into())).await.unwrap();
        }

        let dc_tx;

        loop {
            tokio::select! {
                Some(tx) = dc_rx.recv() => {
                    dc_tx = tx;
                    socket.close(None).await.unwrap();
                    break;
                }
                Some(input) = rx.recv() => {
                    match &input {
                        Msg::IceCandidate(_) => {
                            // relay candidate to peer
                            let json = serde_json::to_string(&input).unwrap();
                            socket.send(Message::Text(json.into())).await.unwrap();
                            println!("ice candidate sent");
                        }
                        _ => {}
                    }
                }
                Some(Ok(msg)) = socket.next() => {
                    match msg {
                        Message::Text(bytes) => {
                            let peer_msg: Msg = serde_json::from_str(bytes.as_str()).unwrap();
                            match peer_msg {
                                Msg::Offer(offer) => {
                                    println!("offer received");
                                    // save as remote description
                                    pc.set_remote_description(offer).await.unwrap();

                                    // create answer
                                    let answer = pc.create_answer(None).await.unwrap();
                                    pc.set_local_description(answer.clone()).await.unwrap();

                                    // send the answer
                                    let answer_msg = Msg::Answer(answer);
                                    let json = serde_json::to_string(&answer_msg).unwrap();
                                    socket.send(Message::Text(json.into())).await.unwrap();
                                    println!("answer sent");
                                }
                                Msg::Answer(answer) => {
                                    println!("answer received");
                                    // save as remote description
                                    pc.set_remote_description(answer).await.unwrap();
                                }
                                Msg::IceCandidate(candidate) => {
                                    println!("candidate received");
                                    // add candidate to peer connection
                                    pc.add_ice_candidate(candidate).await.unwrap();
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // data channel opened

        // receive on the data channel and send to app
        let mut out_clone = output.clone();
        tokio::spawn(async move {
            if initiator {
                out_clone.send(crate::Message::Initiator).await.unwrap();
            }
            while let Some(msg) = rx.recv().await {
                match msg {
                    Msg::NewGame(side) => {
                        out_clone.send(crate::Message::NewGame(side)).await.unwrap();
                    }
                    Msg::UciMove(uci) => {
                        out_clone.send(crate::Message::UciMove(uci)).await.unwrap();
                    }
                    _ => {}
                }
            }
        });

        // receive from the app and send on the data channel
        while let Some(msg) = app_rx.recv().await {
            match msg {
                crate::Message::NewGame(s) => {
                    if let Err(e) = dc_tx.send(Msg::NewGame(s)).await {
                        dbg!(e);
                        break;
                    }
                }
                crate::Message::UciMove(s) => {
                    if let Err(e) = dc_tx.send(Msg::UciMove(s)).await {
                        dbg!(e);
                        break;
                    }
                }
                _ => {}
            }
        }

        output.send(crate::Message::Disconnected).await.unwrap();
    })
}
