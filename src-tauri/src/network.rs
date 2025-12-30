use crate::PiInfo;
use anyhow::{anyhow, Result};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::Duration;

/// Découvre le Raspberry Pi sur le réseau local
pub async fn discover_raspberry_pi(hostname: &str, timeout_secs: u64) -> Result<Option<PiInfo>> {
    let timeout = Duration::from_secs(timeout_secs);
    let start = std::time::Instant::now();

    // Méthode 1: mDNS (Bonjour/Avahi)
    if let Some(info) = discover_via_mdns(hostname).await? {
        return Ok(Some(info));
    }

    // Méthode 2: Scan du réseau local
    while start.elapsed() < timeout {
        if let Some(info) = scan_local_network(hostname).await? {
            return Ok(Some(info));
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    Ok(None)
}

/// Découverte via mDNS (hostname.local)
async fn discover_via_mdns(hostname: &str) -> Result<Option<PiInfo>> {
    use mdns_sd::{ServiceDaemon, ServiceEvent};

    let mdns = ServiceDaemon::new()?;

    // Chercher le service SSH
    let service_type = "_ssh._tcp.local.";
    let receiver = mdns.browse(service_type)?;

    let timeout = Duration::from_secs(10);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                if info.get_hostname().starts_with(hostname) {
                    let ip = info
                        .get_addresses()
                        .iter()
                        .find(|addr| addr.is_ipv4())
                        .map(|addr| addr.to_string());

                    if let Some(ip) = ip {
                        return Ok(Some(PiInfo {
                            ip,
                            hostname: hostname.to_string(),
                            mac_address: None,
                        }));
                    }
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    // Essayer aussi la résolution DNS directe
    let full_hostname = format!("{}.local", hostname);
    if let Ok(addrs) = tokio::net::lookup_host(format!("{}:22", full_hostname)).await {
        for addr in addrs {
            if let IpAddr::V4(ipv4) = addr.ip() {
                if is_ssh_available(&ipv4.to_string()).await {
                    return Ok(Some(PiInfo {
                        ip: ipv4.to_string(),
                        hostname: hostname.to_string(),
                        mac_address: None,
                    }));
                }
            }
        }
    }

    Ok(None)
}

/// Scan le réseau local pour trouver le Pi
async fn scan_local_network(hostname: &str) -> Result<Option<PiInfo>> {
    // Obtenir la plage IP locale
    let local_ip = get_local_ip()?;
    let network_prefix = local_ip.rsplit_once('.').map(|(prefix, _)| prefix).unwrap_or("192.168.1");

    // Scanner les IPs de 1 à 254
    let mut handles = Vec::new();

    for i in 1..=254 {
        let ip = format!("{}.{}", network_prefix, i);
        let hostname = hostname.to_string();

        let handle = tokio::spawn(async move {
            if is_ssh_available(&ip).await {
                // Vérifier si c'est bien notre Pi en essayant de se connecter
                if let Ok(real_hostname) = get_hostname_via_ssh(&ip).await {
                    if real_hostname.contains(&hostname) {
                        return Some(PiInfo {
                            ip,
                            hostname: real_hostname,
                            mac_address: None,
                        });
                    }
                }
            }
            None
        });

        handles.push(handle);
    }

    // Attendre tous les résultats
    for handle in handles {
        if let Ok(Some(info)) = handle.await {
            return Ok(Some(info));
        }
    }

    Ok(None)
}

/// Vérifie si SSH est disponible sur une IP
async fn is_ssh_available(ip: &str) -> bool {
    let addr: SocketAddr = format!("{}:22", ip).parse().unwrap();
    TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok()
}

/// Obtient le hostname via une commande SSH basique
async fn get_hostname_via_ssh(ip: &str) -> Result<String> {
    // On ne peut pas vraiment faire ça sans les credentials
    // Cette fonction est placeholder
    Ok(String::new())
}

/// Obtient l'IP locale de la machine
fn get_local_ip() -> Result<String> {
    use std::net::UdpSocket;

    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect("8.8.8.8:80")?;
    let local_addr = socket.local_addr()?;

    Ok(local_addr.ip().to_string())
}

/// Ping une IP pour vérifier si elle est en ligne
pub async fn ping(ip: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        use tokio::process::Command;
        let output = Command::new("ping")
            .args(["-c", "1", "-W", "1", ip])
            .output()
            .await;
        output.map(|o| o.status.success()).unwrap_or(false)
    }

    #[cfg(target_os = "windows")]
    {
        use tokio::process::Command;
        let output = Command::new("ping")
            .args(["-n", "1", "-w", "1000", ip])
            .output()
            .await;
        output.map(|o| o.status.success()).unwrap_or(false)
    }

    #[cfg(target_os = "linux")]
    {
        use tokio::process::Command;
        let output = Command::new("ping")
            .args(["-c", "1", "-W", "1", ip])
            .output()
            .await;
        output.map(|o| o.status.success()).unwrap_or(false)
    }
}
