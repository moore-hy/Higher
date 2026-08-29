//! DEV-SYNC-003 · QR Pairing：payload 与 LAN IP 候选枚举。
//!
//! 二维码只承载「发现 + 连接参数 + 一次性配对授权」；业务数据仍走 LAN TCP Sync。
//! 禁止进入 payload：API Key / AI Provider Key / Brave Key / password / 数据库内容。

use std::net::Ipv4Addr;

use serde::{Deserialize, Serialize};

pub const QR_PROTOCOL: &str = "higher-sync";
pub const QR_VERSION: u32 = 1;
pub const QR_TTL_SECS: i64 = 10 * 60;
/// candidate 尝试上限（防超大列表拖慢扫码连接）。
pub const MAX_CANDIDATES: usize = 6;
/// 单个 candidate 的 TCP 连接超时（§六：2~3 秒）。
pub const CONNECT_TIMEOUT_SECS: u64 = 2;
pub const CONNECT_TIMEOUT_MS: u64 = 2500;

/// versioned 二维码 payload（JSON 编码进 QR）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QrPairingPayload {
    pub protocol: String,
    pub version: u32,
    pub device_id: String,
    pub device_name: String,
    pub platform: String,
    pub port: u16,
    /// 按优先级排序的本机私网 IPv4 候选（§五）。
    pub candidate_ips: Vec<String>,
    /// 高熵一次性配对 token（uuid v4，122-bit），不再依赖 6 位数字作认证凭据。
    pub pairing_token: String,
    /// unix 秒。过期后二维码作废，必须重新生成。
    pub expires_at: i64,
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// §五 候选分类：Some(优先级 1=192.168/16, 2=10/8, 3=172.16/12)；None = 排除。
/// 排除：loopback / unspecified / link-local(169.254/16) / benchmark(198.18/15) /
/// reserved(240/4) / multicast(224/4) / CGNAT(100.64/10) 及一切非私网公网地址。
pub fn classify_candidate(ip: &Ipv4Addr) -> Option<u8> {
    let o = ip.octets();
    if ip.is_loopback() || ip.is_unspecified() || ip.is_broadcast() || ip.is_multicast() {
        return None;
    }
    match (o[0], o[1]) {
        // 240.0.0.0/4 reserved（含 255.255.255.255）
        (240..=255, _) => None,
        // 224.0.0.0/4 multicast（is_multicast 已含，双保险）
        (224..=239, _) => None,
        // 198.18.0.0/15 benchmark（QR-TC003）
        (198, 18..=19) => None,
        // 169.254.0.0/16 link-local
        (169, 254) => None,
        // 100.64.0.0/10 CGNAT（运营商 NAT，同 Wi-Fi 手机不可达）
        (100, 64..=127) => None,
        // 私网三类（按 §五 优先级）
        (192, 168) => Some(1),
        (10, _) => Some(2),
        (172, 16..=31) => Some(3),
        // 其余（公网等）不进入候选
        _ => None,
    }
}

/// 过滤 + 去重 + 按私网优先级排序；cap 到 MAX_CANDIDATES。
pub fn filter_candidates<S: AsRef<str>>(ips: Vec<S>) -> Vec<String> {
    let mut scored: Vec<(u8, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for raw in ips {
        let s = raw.as_ref().trim().to_string();
        if s.is_empty() {
            continue;
        }
        let Ok(ip) = s.parse::<Ipv4Addr>() else {
            continue;
        };
        let Some(priority) = classify_candidate(&ip) else {
            continue;
        };
        if seen.insert(ip) {
            scored.push((priority, ip.to_string()));
        }
    }
    scored.sort_by_key(|(p, _)| *p);
    scored.into_iter().map(|(_, ip)| ip).take(MAX_CANDIDATES).collect()
}

/// 常见虚拟/隧道网卡关键字（粗滤，§五：不做大型工程）。
pub fn looks_like_virtual_adapter(desc: &str) -> bool {
    const KEYS: [&str; 11] = [
        "tun", "tap", "vpn", "vmware", "virtualbox", "virtual box", "hyper-v", "hns",
        "wsl", "docker", "loopback",
    ];
    let lower = desc.to_ascii_lowercase();
    KEYS.iter().any(|k| lower.contains(k))
}

/// 枚举本机私网 IPv4 候选。
/// Windows：ipconfig 适配器枚举（按适配器描述粗滤虚拟网卡）；
/// 其余平台 / 兜底：UDP connect 探测默认路由出口。
pub fn enumerate_candidate_ips() -> Vec<String> {
    let mut raw: Vec<String> = Vec::new();

    #[cfg(windows)]
    {
        if let Ok(adapters) = ipconfig::get_adapters() {
            for a in adapters {
                let desc = format!("{} {}", a.friendly_name(), a.description());
                if looks_like_virtual_adapter(&desc) {
                    continue;
                }
                for ip in a.ip_addresses() {
                    raw.push(ip.to_string());
                }
            }
        }
    }

    let filtered = filter_candidates(raw);
    if !filtered.is_empty() {
        return filtered;
    }

    // 兜底：UDP connect（不发包）取路由出口地址
    let mut fallback: Vec<String> = Vec::new();
    for target in ["8.8.8.8:80", "1.1.1.1:80", "192.168.1.1:80"] {
        if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0")
            .and_then(|s| {
                s.connect(target)?;
                Ok(s)
            })
        {
            if let Ok(addr) = sock.local_addr() {
                fallback.push(addr.ip().to_string());
            }
        }
    }
    filter_candidates(fallback)
}

/// 组装 QR payload JSON（server 侧调用：token/expires 由 session 提供）。
pub fn build_payload_json(
    device_id: &str,
    device_name: &str,
    port: u16,
    candidate_ips: Vec<String>,
    pairing_token: &str,
    expires_at: i64,
) -> Result<String, String> {
    let payload = QrPairingPayload {
        protocol: QR_PROTOCOL.to_string(),
        version: QR_VERSION,
        device_id: device_id.to_string(),
        device_name: device_name.to_string(),
        platform: "windows".to_string(),
        port,
        candidate_ips,
        pairing_token: pairing_token.to_string(),
        expires_at,
    };
    serde_json::to_string(&payload).map_err(|e| e.to_string())
}

/// 校验扫码得到的 payload（§六/§十三：分类错误消息，禁止卡死）。
pub fn parse_payload(json: &str) -> Result<QrPairingPayload, String> {
    let p: QrPairingPayload =
        serde_json::from_str(json).map_err(|_| "二维码格式错误：不是有效的 Higher 配对码".to_string())?;
    if p.protocol != QR_PROTOCOL {
        return Err("非 Higher 二维码：请扫描电脑 Higher「同步 → 添加手机」显示的二维码".into());
    }
    if p.version != QR_VERSION {
        return Err(format!("协议版本不兼容（二维码 v{}，本机支持 v{QR_VERSION}）", p.version));
    }
    if p.expires_at <= now_unix() {
        return Err("二维码已过期：请在电脑上刷新二维码后重新扫描".into());
    }
    if p.port == 0 {
        return Err("二维码缺少有效端口".into());
    }
    if p.candidate_ips.is_empty() {
        return Err("二维码未包含可用的电脑地址".into());
    }
    Ok(p)
}
