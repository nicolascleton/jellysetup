use crate::PiInfo;
use anyhow::Result;
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

/// Helper pour logger dans un fichier
fn log_to_file(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/jellysetup_discovery.log") {
        let _ = writeln!(f, "{}", msg);
    }
}

/// Découverte via mDNS (hostname.local)
async fn discover_via_mdns(hostname: &str) -> Result<Option<PiInfo>> {
    log_to_file(&format!("discover_via_mdns START for {}.local", hostname));

    // Méthode SIMPLE: ping et extraire l'IP
    #[cfg(target_os = "macos")]
    {
        use tokio::process::Command;
        let full_hostname = format!("{}.local", hostname);

        log_to_file(&format!("Ping {}...", full_hostname));
        // IMPORTANT: Utiliser le chemin absolu car le PATH des apps GUI ne contient pas /sbin
        let ping_result = Command::new("/sbin/ping")
            .args(["-c", "1", "-W", "3", &full_hostname])
            .output()
            .await;

        match ping_result {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                log_to_file(&format!("Ping exit status: {:?}", output.status));
                log_to_file(&format!("Ping stdout: {}", stdout));
                if !stderr.is_empty() {
                    log_to_file(&format!("Ping stderr: {}", stderr));
                }

                // Format: "PING jellypi.local (192.168.1.106): 56 data bytes"
                if let Some(line) = stdout.lines().next() {
                    log_to_file(&format!("First line: {}", line));
                    if let Some(start) = line.find('(') {
                        if let Some(end) = line.find(')') {
                            let ip_str = &line[start + 1..end];
                            log_to_file(&format!("Extracted IP: {}", ip_str));

                            // Vérifier SSH avec nc (plus fiable)
                            // IMPORTANT: Utiliser le chemin absolu
                            let nc_result = Command::new("/usr/bin/nc")
                                .args(["-z", "-w", "2", ip_str, "22"])
                                .output()
                                .await;

                            if nc_result.map(|o| o.status.success()).unwrap_or(false) {
                                log_to_file(&format!("SSH OK on {}", ip_str));
                                return Ok(Some(PiInfo {
                                    ip: ip_str.to_string(),
                                    hostname: hostname.to_string(),
                                    mac_address: None,
                                }));
                            } else {
                                log_to_file("SSH check failed, returning IP anyway");
                                // Retourner l'IP quand même, on vérifiera SSH plus tard
                                return Ok(Some(PiInfo {
                                    ip: ip_str.to_string(),
                                    hostname: hostname.to_string(),
                                    mac_address: None,
                                }));
                            }
                        } else {
                            log_to_file("Could not find closing ')' in ping output");
                        }
                    } else {
                        log_to_file("Could not find '(' in ping output");
                    }
                } else {
                    log_to_file("No lines in ping output");
                }
            }
            Err(e) => {
                log_to_file(&format!("Ping command failed to execute: {:?}", e));
                log_to_file("This usually means /sbin/ping is not accessible from GUI app");
            }
        }
    }

    // Méthode 1bis: Résolution DNS standard (pour autres OS)
    #[cfg(not(target_os = "macos"))]
    {
        let full_hostname = format!("{}.local", hostname);
        if let Ok(addrs) = tokio::net::lookup_host(format!("{}:22", full_hostname)).await {
            for addr in addrs {
                if let IpAddr::V4(ipv4) = addr.ip() {
                    let ip_str = ipv4.to_string();
                    println!("[Discovery] Resolved {} to {}", full_hostname, ip_str);
                    if is_ssh_available(&ip_str).await {
                        println!("[Discovery] SSH available on {}", ip_str);
                        return Ok(Some(PiInfo {
                            ip: ip_str,
                            hostname: hostname.to_string(),
                            mac_address: None,
                        }));
                    }
                }
            }
        }
    }

    // Méthode 2: mDNS service discovery (backup)
    use mdns_sd::{ServiceDaemon, ServiceEvent};

    if let Ok(mdns) = ServiceDaemon::new() {
        let service_type = "_ssh._tcp.local.";
        if let Ok(receiver) = mdns.browse(service_type) {
            let timeout = Duration::from_secs(5);
            let start = std::time::Instant::now();

            while start.elapsed() < timeout {
                match receiver.recv_timeout(Duration::from_secs(1)) {
                    Ok(ServiceEvent::ServiceResolved(info)) => {
                        println!("[Discovery] mDNS found: {}", info.get_hostname());
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
async fn get_hostname_via_ssh(_ip: &str) -> Result<String> {
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
        // Chemin absolu pour les apps GUI
        let output = Command::new("/sbin/ping")
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
