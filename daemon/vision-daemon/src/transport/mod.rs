//! OS-native local transports for the Local API Gateway
//! (`docs/ARCHITECTURE.md` §4.1): named pipe on Windows, Unix domain
//! socket on macOS/Linux. Never a network-exposed port.

#[cfg(windows)]
pub mod windows;
