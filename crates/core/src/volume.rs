use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Replacement {
    Exchange,
    RenameAfterALastLook,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Creation {
    RenameWithoutReplacing,
    HardLink,
    CreateNew,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Volume {
    pub replacement: Replacement,
    pub creation: Creation,
}

impl Volume {
    pub const WITHOUT_EXCHANGE: Volume = Volume {
        replacement: Replacement::RenameAfterALastLook,
        creation: Creation::CreateNew,
    };

    #[cfg(any(test, feature = "volume-seam"))]
    pub const WITH_HARD_LINKS_ONLY: Volume = Volume {
        replacement: Replacement::RenameAfterALastLook,
        creation: Creation::HardLink,
    };

    pub(crate) fn probe(one: &Path, other: &Path) -> io::Result<Volume> {
        let volume = Volume {
            replacement: probe_exchange(one, other)?,
            creation: probe_creation(one, other)?,
        };
        let _ = fs::remove_file(one);
        let _ = fs::remove_file(other);
        Ok(volume)
    }
}

fn fresh(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let _ = fs::remove_file(path);
    fs::write(path, bytes)
}

fn holds(path: &Path, bytes: &[u8]) -> bool {
    fs::read(path).is_ok_and(|held| held == bytes)
}

fn probe_exchange(one: &Path, other: &Path) -> io::Result<Replacement> {
    fresh(one, b"one")?;
    fresh(other, b"other")?;
    match platform::exchange(one, other) {
        Ok(()) if holds(one, b"other") && holds(other, b"one") => Ok(Replacement::Exchange),
        Ok(()) => Ok(Replacement::RenameAfterALastLook),
        Err(error) if platform::is_missing(&error) => Ok(Replacement::RenameAfterALastLook),
        Err(error) => Err(error),
    }
}

fn probe_creation(one: &Path, other: &Path) -> io::Result<Creation> {
    if never_replaces(one, other, platform::rename_without_replacing)? {
        Ok(Creation::RenameWithoutReplacing)
    } else if never_replaces(one, other, |from, to| fs::hard_link(from, to))? {
        Ok(Creation::HardLink)
    } else {
        Ok(Creation::CreateNew)
    }
}

fn never_replaces(
    one: &Path,
    other: &Path,
    create: impl Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<bool> {
    fresh(one, b"first")?;
    let _ = fs::remove_file(other);
    match create(one, other) {
        Ok(()) => {}
        Err(error) if platform::is_missing(&error) => return Ok(false),
        Err(error) => return Err(error),
    }
    if !holds(other, b"first") {
        return Ok(false);
    }
    fresh(one, b"second")?;
    match create(one, other) {
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(holds(other, b"first")),
        Ok(()) => Ok(false),
        Err(error) if platform::is_missing(&error) => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) mod platform {
    use std::io;
    use std::path::Path;

    use rustix::fs::{CWD, RenameFlags, renameat_with};
    use rustix::io::Errno;

    const MISSING: [Errno; 5] = [
        Errno::NOTSUP,
        Errno::OPNOTSUPP,
        Errno::INVAL,
        Errno::NOSYS,
        Errno::PERM,
    ];

    pub fn exchange(one: &Path, other: &Path) -> io::Result<()> {
        Ok(renameat_with(CWD, one, CWD, other, RenameFlags::EXCHANGE)?)
    }

    pub fn rename_without_replacing(from: &Path, to: &Path) -> io::Result<()> {
        Ok(renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE)?)
    }

    pub fn is_missing(error: &io::Error) -> bool {
        Errno::from_io_error(error).is_some_and(|errno| MISSING.contains(&errno))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) mod platform {
    use std::io::{self, ErrorKind};
    use std::path::Path;

    pub fn exchange(_one: &Path, _other: &Path) -> io::Result<()> {
        Err(ErrorKind::Unsupported.into())
    }

    pub fn rename_without_replacing(_from: &Path, _to: &Path) -> io::Result<()> {
        Err(ErrorKind::Unsupported.into())
    }

    pub fn is_missing(error: &io::Error) -> bool {
        error.kind() == ErrorKind::Unsupported
    }
}
