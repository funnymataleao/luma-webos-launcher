//! The webOS SDK's libc lacks getauxval, used by Rust's startup and ring.
//! Read real kernel auxiliary values rather than inventing CPU capabilities.

fn read_auxv(fd: libc::c_int, kind: libc::c_ulong) -> Option<libc::c_ulong> {
    loop {
        let mut entry = [0 as libc::c_ulong; 2];
        // SAFETY: the byte slice covers exactly this initialized, writable entry.
        let bytes = unsafe {
            std::slice::from_raw_parts_mut(
                entry.as_mut_ptr().cast::<u8>(),
                std::mem::size_of_val(&entry),
            )
        };
        let mut offset = 0;
        while offset < bytes.len() {
            // SAFETY: read receives a valid writable slice and never owns fd.
            let count = unsafe {
                libc::read(
                    fd,
                    bytes[offset..].as_mut_ptr().cast(),
                    bytes.len() - offset,
                )
            };
            if count < 0 {
                if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return None;
            }
            if count == 0 {
                return None;
            }
            offset += count as usize;
        }
        if entry[0] == 0 {
            return None;
        }
        if entry[0] == kind {
            return Some(entry[1]);
        }
    }
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
extern "C" fn getauxval(kind: libc::c_ulong) -> libc::c_ulong {
    // SAFETY: errno is thread-local; open uses a static NUL-terminated pathname.
    // This path needs no allocations, SDL, or initialized Rust application state.
    unsafe {
        let errno = libc::__errno_location();
        let saved_errno = *errno;
        let fd = libc::open(
            c"/proc/self/auxv".as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC,
        );
        let value = if fd >= 0 {
            let value = read_auxv(fd, kind);
            libc::close(fd);
            value
        } else {
            None
        };
        *errno = if value.is_some() {
            saved_errno
        } else {
            libc::ENOENT
        };
        value.unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, os::fd::AsRawFd, os::unix::net::UnixStream};

    fn lookup(
        entries: &[[libc::c_ulong; 2]],
        truncate: usize,
        key: libc::c_ulong,
    ) -> Option<libc::c_ulong> {
        let (reader, mut writer) = UnixStream::pair().unwrap();
        let bytes: Vec<u8> = entries
            .iter()
            .flatten()
            .flat_map(|n| n.to_ne_bytes())
            .collect();
        writer.write_all(&bytes[..bytes.len() - truncate]).unwrap();
        writer.shutdown(std::net::Shutdown::Write).unwrap();
        read_auxv(reader.as_raw_fd(), key)
    }

    #[test]
    fn preserves_kernel_values_including_zero() {
        let entries = [[6, 4096], [16, 0], [26, 17], [0, 0]];
        assert_eq!(lookup(&entries, 0, 6), Some(4096));
        assert_eq!(lookup(&entries, 0, 16), Some(0));
        assert_eq!(lookup(&entries, 0, 26), Some(17));
        assert_eq!(lookup(&entries, 0, 42), None);
    }

    #[test]
    fn rejects_terminator_and_incomplete_records() {
        assert_eq!(lookup(&[[0, 0], [6, 4096]], 0, 6), None);
        assert_eq!(lookup(&[[0, 0]], 0, 0), None);
        assert_eq!(lookup(&[[6, 4096]], 1, 6), None);
        assert_eq!(lookup(&[], 0, 6), None);
        assert_eq!(read_auxv(-1, 6), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn exposes_real_page_size_and_libc_errno_contract() {
        unsafe {
            *libc::__errno_location() = libc::EACCES;
            assert_eq!(
                getauxval(libc::AT_PAGESZ),
                libc::sysconf(libc::_SC_PAGESIZE) as libc::c_ulong
            );
            assert_eq!(*libc::__errno_location(), libc::EACCES);
            assert_eq!(getauxval(libc::c_ulong::MAX), 0);
            assert_eq!(*libc::__errno_location(), libc::ENOENT);
        }
    }
}
