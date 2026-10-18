//! Loopback HTTP CONNECT -> authenticated SOCKS5. No HTTP status based rotation.
use apw_core::config::{SocksNode, SocksSettings};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;

static SETTINGS: Mutex<Option<SocksSettings>> = Mutex::new(None);
static PORT: OnceLock<u16> = OnceLock::new();
static START: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
pub fn port() -> u16 {
    PORT.get().copied().unwrap_or(0)
}

pub async fn configure(settings: SocksSettings) -> Result<(), String> {
    if settings.nodes.len() > 100 || settings.selected >= settings.nodes.len().max(1) {
        return Err("节点数量或首选节点无效".into());
    }
    *SETTINGS.lock().unwrap() = Some(settings);
    let _guard = START.lock().await;
    if PORT.get().is_some() {
        return Ok(());
    }
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|_| "无法启动本机代理")?;
    let bound_port = listener
        .local_addr()
        .map_err(|_| "无法读取本机代理端口")?
        .port();
    PORT.set(bound_port).map_err(|_| "代理已启动")?;
    tokio::spawn(async move {
        let limit = Arc::new(Semaphore::new(32));
        while let Ok((stream, _)) = listener.accept().await {
            let Ok(permit) = limit.clone().try_acquire_owned() else {
                continue;
            };
            tokio::spawn(async move {
                let _permit = permit;
                let _ = serve(stream).await;
            });
        }
    });
    Ok(())
}

// Errors deliberately omit upstream addresses, usernames and passwords.
async fn socks(node: &SocksNode, target: &str, port: u16) -> Result<TcpStream, &'static str> {
    if node.host.is_empty()
        || node.port == 0
        || node.username.len() > 255
        || node.password.len() > 255
        || target.len() > 255
    {
        return Err("节点格式无效");
    }
    let mut stream = TcpStream::connect((node.host.as_str(), node.port))
        .await
        .map_err(|_| "节点连接失败")?;
    let auth = !node.username.is_empty() || !node.password.is_empty();
    stream
        .write_all(&[5, 1, if auth { 2 } else { 0 }])
        .await
        .map_err(|_| "握手失败")?;
    let mut reply = [0; 2];
    stream
        .read_exact(&mut reply)
        .await
        .map_err(|_| "握手失败")?;
    if reply != [5, if auth { 2 } else { 0 }] {
        return Err("认证方式不支持");
    }
    if auth {
        let mut packet = vec![1, node.username.len() as u8];
        packet.extend(node.username.as_bytes());
        packet.push(node.password.len() as u8);
        packet.extend(node.password.as_bytes());
        stream.write_all(&packet).await.map_err(|_| "认证失败")?;
        stream
            .read_exact(&mut reply)
            .await
            .map_err(|_| "认证失败")?;
        if reply != [1, 0] {
            return Err("认证失败");
        }
    }
    let mut packet = vec![5, 1, 0, 3, target.len() as u8];
    packet.extend(target.as_bytes());
    packet.extend(port.to_be_bytes());
    stream.write_all(&packet).await.map_err(|_| "隧道失败")?;
    let mut head = [0; 4];
    stream.read_exact(&mut head).await.map_err(|_| "隧道失败")?;
    // A destination rejection must never cause failover.
    if head[0] != 5 || head[1] != 0 {
        return Err("目标连接被拒绝");
    }
    let size = match head[3] {
        1 => 4,
        4 => 16,
        3 => stream.read_u8().await.map_err(|_| "隧道失败")? as usize,
        _ => return Err("隧道失败"),
    };
    let mut tail = vec![0; size + 2];
    stream.read_exact(&mut tail).await.map_err(|_| "隧道失败")?;
    Ok(stream)
}

async fn connect(node: &SocksNode, host: &str, port: u16) -> Result<TcpStream, &'static str> {
    tokio::time::timeout(Duration::from_secs(8), socks(node, host, port))
        .await
        .unwrap_or(Err("节点连接超时"))
}

async fn serve(mut client: TcpStream) -> Result<(), &'static str> {
    let header = tokio::time::timeout(Duration::from_secs(5), async {
        let mut bytes = Vec::new();
        while bytes.len() < 8192 {
            bytes.push(client.read_u8().await.map_err(|_| "请求读取失败")?);
            if bytes.ends_with(b"\r\n\r\n") {
                return Ok(bytes);
            }
        }
        Err("请求过长")
    })
    .await
    .map_err(|_| "请求超时")??;
    let text = std::str::from_utf8(&header).map_err(|_| "请求格式错误")?;
    let mut words = text.split_whitespace();
    if words.next() != Some("CONNECT") {
        return Err("仅支持 HTTPS CONNECT");
    }
    let (host, port) = words
        .next()
        .and_then(|s| s.rsplit_once(':'))
        .ok_or("目标格式错误")?;
    let port: u16 = port.parse().map_err(|_| "目标端口错误")?;
    // This is a private Apple query bridge, never a general-purpose local proxy.
    if port != 443
        || !(host == "apple.com"
            || host.ends_with(".apple.com")
            || host == "apple.com.cn"
            || host.ends_with(".apple.com.cn"))
    {
        return Err("目标不允许");
    }
    let settings = SETTINGS.lock().unwrap().clone().ok_or("未配置节点")?;
    if settings.nodes.is_empty() || settings.selected >= settings.nodes.len() {
        return Err("请选择有效节点");
    }
    let count = if settings.failover {
        settings.nodes.len()
    } else {
        1
    };
    for offset in 0..count {
        let index = (settings.selected + offset) % settings.nodes.len();
        match connect(&settings.nodes[index], host, port).await {
            Ok(mut upstream) => {
                // Remember working standby only if configuration has not changed meanwhile.
                if offset > 0 {
                    let mut current = SETTINGS.lock().unwrap();
                    if current.as_ref() == Some(&settings) {
                        current.as_mut().unwrap().selected = index;
                    }
                }
                client
                    .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                    .await
                    .map_err(|_| "响应失败")?;
                // Once tunneled, EOF/HTTP errors never retry on another IP.
                let _ = tokio::time::timeout(
                    Duration::from_secs(120),
                    tokio::io::copy_bidirectional(&mut client, &mut upstream),
                )
                .await;
                return Ok(());
            }
            Err("节点连接失败" | "节点连接超时" | "握手失败") => continue,
            Err(error) => return Err(error),
        }
    }
    let _ = client
        .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
        .await;
    Err("所有节点不可连接")
}

#[tauri::command]
pub async fn test_socks_node(node: SocksNode) -> Result<u128, String> {
    let start = Instant::now();
    // Complete SOCKS negotiation and a TLS-port TCP tunnel; no Apple HTTP request.
    connect(&node, "www.apple.com", 443)
        .await
        .map_err(str::to_string)?;
    Ok(start.elapsed().as_millis())
}

#[tauri::command]
pub fn socks_active_node() -> Option<usize> {
    SETTINGS.lock().unwrap().as_ref().and_then(|s| {
        if s.nodes.is_empty() {
            None
        } else {
            Some(s.selected)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn fake(auth_status: u8) -> (SocksNode, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let node = SocksNode {
            host: "127.0.0.1".into(),
            port: listener.local_addr().unwrap().port(),
            username: "user".into(),
            password: "secret:pass".into(),
        };
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut greeting = [0; 3];
            stream.read_exact(&mut greeting).await.unwrap();
            assert_eq!(greeting, [5, 1, 2]);
            stream.write_all(&[5, 2]).await.unwrap();
            assert_eq!(stream.read_u8().await.unwrap(), 1);
            let size = stream.read_u8().await.unwrap() as usize;
            let mut username = vec![0; size];
            stream.read_exact(&mut username).await.unwrap();
            assert_eq!(username, b"user");
            let size = stream.read_u8().await.unwrap() as usize;
            let mut password = vec![0; size];
            stream.read_exact(&mut password).await.unwrap();
            assert_eq!(password, b"secret:pass");
            stream.write_all(&[1, auth_status]).await.unwrap();
            if auth_status != 0 {
                return;
            }
            let mut head = [0; 5];
            stream.read_exact(&mut head).await.unwrap();
            assert_eq!(&head[..4], &[5, 1, 0, 3]);
            let mut domain = vec![0; head[4] as usize];
            stream.read_exact(&mut domain).await.unwrap();
            assert_eq!(domain, b"www.apple.com");
            assert_eq!(stream.read_u16().await.unwrap(), 443);
            stream
                .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 1, 187])
                .await
                .unwrap();
        });
        (node, task)
    }
    #[tokio::test]
    async fn authenticated_socks_tunnel_and_remote_dns() {
        let (node, task) = fake(0).await;
        assert!(connect(&node, "www.apple.com", 443).await.is_ok());
        task.await.unwrap();
        assert!(!format!("{node:?}").contains("secret"));
    }
    #[tokio::test]
    async fn authentication_failure_is_not_a_network_failure() {
        let (node, task) = fake(1).await;
        assert_eq!(
            connect(&node, "www.apple.com", 443).await.unwrap_err(),
            "认证失败"
        );
        task.await.unwrap();
    }
    #[tokio::test]
    async fn fallback_only_before_tunnel_and_no_direct_connection() {
        let (node, task) = fake(0).await;
        let closed = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let closed_port = closed.local_addr().unwrap().port();
        drop(closed);
        let mut dead = node.clone();
        dead.port = closed_port;
        configure(SocksSettings {
            nodes: vec![dead, node],
            selected: 0,
            failover: true,
        })
        .await
        .unwrap();
        let mut browser = TcpStream::connect(("127.0.0.1", port())).await.unwrap();
        browser
            .write_all(b"CONNECT www.apple.com:443 HTTP/1.1\r\nHost: www.apple.com:443\r\n\r\n")
            .await
            .unwrap();
        let mut response = [0; 39];
        browser.read_exact(&mut response).await.unwrap();
        assert!(response.starts_with(b"HTTP/1.1 200"));
        assert_eq!(socks_active_node(), Some(1));
        task.await.unwrap();
    }
}
