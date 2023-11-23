
use aws_config::BehaviorVersion;
use std::time::Duration;
use async_stream::stream;
use poem::{
    get,
    listener::{Listener, Acceptor, TcpListener},
    EndpointExt, Route, Server,
};
use tokio::{pin, select};
use tokio::time::sleep;
use tokio_stream::StreamExt;
use tokio_tungstenite::{
    connect_async,
    tungstenite::Message,
};
use futures_util::sink::SinkExt;
use tracing::{info, error, debug};
use polyhedron::{
    stream_single,
    SingleEvent,
    asr::slice_i16_to_u8,
    Context
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[tracing_test::traced_test]
async fn test_single() {
    let shared_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let ctx = Context::new(&shared_config);

    let acceptor = TcpListener::bind("[::]:0")
        .into_acceptor()
        .await
        .unwrap();
    let addr = acceptor
        .local_addr()
        .remove(0)
        .as_socket_addr()
        .cloned()
        .unwrap();
    let server = Server::new_with_acceptor(acceptor);
    let handle = tokio::spawn(async move {
        let _ = server.run(
            Route::new()
                .at("/ws/voice", get(stream_single))
                .data(ctx)
        ).await;
    });

    let url = format!(
        "ws://{}/ws/voice?id=123abc&from=zh-CN&to=en-US&voice=Amy", addr
    );
    let (mut client_stream, _) = connect_async(url)
        .await
        .unwrap();

    client_stream
        .send(Message::Binary(Vec::new()))
        .await
        .unwrap();


    let wav = hound::WavReader::open("whisper/samples/samples_jfk.wav")
        .expect("failed to open wav");
    let spec = wav.spec();
    info!("{:?}", spec);
    let samples = wav
        .into_samples::<i16>()
        .map(|s| s.unwrap())
        .collect::<Vec<i16>>();
    let chunks = samples.chunks(1600)
        .map(|chunk| chunk.to_vec())
        .into_iter();

    let audio_stream = stream! {
        for chunk in chunks {
            yield slice_i16_to_u8(&chunk);
            sleep(Duration::from_millis(10)).await;
        }
    };
    pin!(audio_stream);

    let recv_fut = async {
        while let Some(voice_slice) = audio_stream.next().await {
            client_stream.send(Message::Binary(voice_slice)).await?;
        }
        info!("sent all voice chunks");

        while let Some(Ok(msg)) = client_stream.next().await {
            debug!("recv: {:?}", msg);
            let Message::Text(json_str) = msg else { continue };
            let Ok(evt) = serde_json::from_str::<SingleEvent>(&json_str) else { continue };
            if let SingleEvent::Voice { .. } = evt {
                return Ok(())
            }
        }

        Ok(()) as anyhow::Result<()>
    };

    select! {
        res = recv_fut => {
            if let Err(e) = res {
                error!("Error: {:?}", e);
                assert!(false, "Error: {}", e);
            }
        }
        _ = sleep(Duration::from_secs(10)) => {
            assert!(false, "timeout");
        }
    };

    handle.abort();
}