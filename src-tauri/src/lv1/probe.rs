#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TcpConnectProbeResult {
    pub tcp_connect_ms: u64,
}

pub fn clamp_tcp_probe_timeout(timeout_ms: Option<u64>) -> std::time::Duration {
    std::time::Duration::from_millis(timeout_ms.unwrap_or(500).clamp(100, 2000))
}

pub async fn probe_tcp_connect_latency(
    address: &str,
    port: u16,
    timeout_ms: Option<u64>,
) -> Result<TcpConnectProbeResult, String> {
    let timeout = clamp_tcp_probe_timeout(timeout_ms);
    let target = format!("{address}:{port}");
    let started_at = std::time::Instant::now();
    let stream = tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&target))
        .await
        .map_err(|_| "TCP probe timed out".to_string())?
        .map_err(|err| format!("TCP probe failed: {err}"))?;
    drop(stream);

    Ok(TcpConnectProbeResult {
        tcp_connect_ms: started_at.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_tcp_probe_timeout() {
        assert_eq!(
            clamp_tcp_probe_timeout(None),
            std::time::Duration::from_millis(500)
        );
        assert_eq!(
            clamp_tcp_probe_timeout(Some(25)),
            std::time::Duration::from_millis(100)
        );
        assert_eq!(
            clamp_tcp_probe_timeout(Some(750)),
            std::time::Duration::from_millis(750)
        );
        assert_eq!(
            clamp_tcp_probe_timeout(Some(3000)),
            std::time::Duration::from_millis(2000)
        );
    }

    #[tokio::test]
    async fn probes_successful_tcp_connect_latency() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let accept_task = tokio::spawn(async move {
            let _ = listener.accept().await.unwrap();
        });

        let result = probe_tcp_connect_latency("127.0.0.1", port, Some(500))
            .await
            .unwrap();

        assert!(result.tcp_connect_ms <= 500);
        accept_task.await.unwrap();
    }
}
