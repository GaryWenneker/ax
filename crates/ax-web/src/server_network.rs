//! Private interface choices for explicit LAN advertisement.
use serde::Serialize;
use std::{collections::BTreeSet,net::IpAddr};
#[derive(Serialize)]
pub struct Address {pub name:String,pub address:String,pub loopback:bool}
pub fn private(ip:IpAddr)->bool {match ip {IpAddr::V4(ip)=>ip.is_private() || ip.is_loopback(),IpAddr::V6(ip)=>ip.is_unique_local() || ip.is_loopback()}}
pub fn addresses()->Result<Vec<Address>,String> {
    let mut found=BTreeSet::new();
    if let Ok(interfaces)=if_addrs::get_if_addrs() {for item in interfaces {let ip=item.ip();if private(ip) {found.insert((ip,item.name));}}}
    #[cfg(target_os="linux")]
    if found.is_empty() {
        let text=std::fs::read_to_string("/proc/net/fib_trie").map_err(|e|format!("Cannot list network interfaces: {e}"))?;
        let mut previous=None;
        for line in text.lines() {
            let trimmed=line.trim();
            if let Some(ip)=trimmed.split_whitespace().last().and_then(|v|v.parse::<IpAddr>().ok()) {previous=Some(ip);}
            if trimmed.contains("/32 host LOCAL") {if let Some(ip)=previous.take() {if private(ip) {found.insert((ip,"Local interface".into()));}}}
        }
        if let Ok(text)=std::fs::read_to_string("/proc/net/if_inet6") {for line in text.lines() {
            let fields:Vec<_>=line.split_whitespace().collect(); if fields.len()!=6 || fields[0].len()!=32 {continue;}
            let groups:Vec<_>=fields[0].as_bytes().chunks(4).filter_map(|s|std::str::from_utf8(s).ok()).collect();
            if let Ok(ip)=groups.join(":").parse::<IpAddr>() {if private(ip) {found.insert((ip,fields[5].into()));}}
        }}
    }
    let mut result:Vec<_>=found.into_iter().map(|(ip,name)|Address{name,address:ip.to_string(),loopback:ip.is_loopback()}).collect();
    result.sort_by(|a,b|(a.loopback,&a.address,&a.name).cmp(&(b.loopback,&b.address,&b.name))); result.dedup_by(|a,b|a.address==b.address);
    Ok(result)
}
