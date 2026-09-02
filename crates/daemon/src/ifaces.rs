//! The address to print when the daemon binds `0.0.0.0`.

use std::net::Ipv4Addr;

/// The first IPv4 address of an interface that is up and neither loopback
/// nor link-local, in `getifaddrs` order; `None` when there is none.
#[allow(unsafe_code)] // getifaddrs walks a C list; freed before returning
pub fn first_non_loopback_v4() -> Option<Ipv4Addr> {
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: `list` is a valid out-pointer; a non-zero return leaves it untouched.
    if unsafe { libc::getifaddrs(&raw mut list) } != 0 {
        return None;
    }
    let mut found = None;
    let mut cur = list;
    while !cur.is_null() {
        // SAFETY: every node of the list getifaddrs returned is a valid ifaddrs.
        let ifa = unsafe { &*cur };
        let up = ifa.ifa_flags & libc::IFF_UP as u32 != 0;
        let loopback = ifa.ifa_flags & libc::IFF_LOOPBACK as u32 != 0;
        if up && !loopback && !ifa.ifa_addr.is_null() {
            // SAFETY: ifa_addr is non-null and points at a sockaddr; its
            // family says whether it is the sockaddr_in read below.
            let family = i32::from(unsafe { (*ifa.ifa_addr).sa_family });
            if family == libc::AF_INET {
                // SAFETY: an AF_INET address is a sockaddr_in.
                let sin = unsafe { &*ifa.ifa_addr.cast::<libc::sockaddr_in>() };
                let ip = Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
                if !ip.is_loopback() && !ip.is_link_local() {
                    found = Some(ip);
                    break;
                }
            }
        }
        cur = ifa.ifa_next;
    }
    // SAFETY: `list` came from getifaddrs and is freed exactly once.
    unsafe { libc::freeifaddrs(list) };
    found
}

#[cfg(test)]
mod tests {
    use super::first_non_loopback_v4;

    #[test]
    fn never_returns_loopback() {
        if let Some(ip) = first_non_loopback_v4() {
            assert!(!ip.is_loopback());
            assert!(!ip.is_unspecified());
        }
    }
}
