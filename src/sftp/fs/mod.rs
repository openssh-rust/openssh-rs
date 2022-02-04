use super::{Auxiliary, Error, Id, OwnedHandle, Sftp, WriteEnd, WriteEndWithCachedId};

use std::borrow::Cow;
use std::future::Future;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use openssh_sftp_client::Error as SftpError;

mod dir;
pub use dir::{DirEntry, ReadDir};

/// A struct used to perform operations on remote filesystem.
#[derive(Debug, Clone)]
pub struct Fs<'s> {
    phantom_data: PhantomData<&'s Sftp<'s>>,

    write_end: WriteEndWithCachedId,
    cwd: Box<Path>,
}

impl Fs<'_> {
    pub(super) fn new(write_end: WriteEndWithCachedId, cwd: PathBuf) -> Self {
        Self {
            phantom_data: PhantomData,

            write_end,
            cwd: cwd.into_boxed_path(),
        }
    }

    fn get_auxiliary(&self) -> &Auxiliary {
        self.write_end.get_auxiliary()
    }

    async fn send_request<Func, F, R>(&mut self, f: Func) -> Result<R, Error>
    where
        Func: FnOnce(&mut WriteEnd, Id) -> Result<F, SftpError>,
        F: Future<Output = Result<(Id, R), SftpError>> + 'static,
    {
        let id = self.write_end.get_id_mut();

        let future = f(&mut self.write_end, id)?;

        let (id, ret) = self.get_auxiliary().cancel_if_task_failed(future).await?;

        self.write_end.cache_id_mut(id);

        Ok(ret)
    }

    /// Return current working dir.
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// Set current working dir.
    ///
    /// * `cwd` - Can include `~`.
    pub fn set_cwd(&mut self, cwd: impl Into<PathBuf>) {
        self.cwd = cwd.into().into_boxed_path();
    }

    fn concat_path_if_needed<'path>(&self, path: &'path Path) -> Cow<'path, Path> {
        if path.is_absolute() {
            Cow::Borrowed(path)
        } else {
            Cow::Owned(self.cwd.join(path))
        }
    }

    /// Open a remote dir
    pub async fn open_dir(&mut self, path: impl AsRef<Path>) -> Result<Dir<'_>, Error> {
        let path = path.as_ref();

        let path = self.concat_path_if_needed(path);

        self.send_request(|write_end, id| Ok(write_end.send_opendir_request(id, path)?.wait()))
            .await
            .map(|handle| Dir(OwnedHandle::new(self.write_end.clone(), handle)))
    }
}

/// Remote Directory
#[repr(transparent)]
#[derive(Debug, Clone)]
pub struct Dir<'s>(OwnedHandle<'s>);

impl Dir<'_> {
    /// Read dir.
    pub async fn read_dir(&mut self) -> Result<ReadDir, Error> {
        self.0
            .send_request(|write_end, handle, id| {
                Ok(write_end.send_readdir_request(id, handle)?.wait())
            })
            .await
            .map(ReadDir)
    }

    /// Close dir.
    pub async fn close(self) -> Result<(), Error> {
        self.0.close().await
    }
}
