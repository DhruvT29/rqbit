use std::path::PathBuf;

use anyhow::{Context, bail};
use librqbit_core::Id20;

use crate::{
    api::TorrentIdOrHash,
    bitv::{BitV, DiskBackedBitV},
    spawn_utils::BlockingSpawner,
    type_aliases::BF,
};

#[async_trait::async_trait]
pub trait BitVFactory: Send + Sync {
    async fn load(&self, id: TorrentIdOrHash) -> anyhow::Result<Option<Box<dyn BitV>>>;
    async fn clear(&self, id: TorrentIdOrHash) -> anyhow::Result<()>;
    async fn store_initial_check(
        &self,
        id: TorrentIdOrHash,
        b: BF,
    ) -> anyhow::Result<Box<dyn BitV>>;
}

/// Each torrent's bitfield in a file of its own, named by its info hash, with
/// no session store behind it: fast resume, and nothing that re-adds torrents.
pub struct FolderBitVFactory {
    folder: PathBuf,
    spawner: BlockingSpawner,
}

impl FolderBitVFactory {
    pub async fn new(folder: PathBuf, spawner: BlockingSpawner) -> anyhow::Result<Self> {
        tokio::fs::create_dir_all(&folder)
            .await
            .with_context(|| format!("error creating {folder:?}"))?;
        Ok(Self { folder, spawner })
    }

    fn filename(&self, id: TorrentIdOrHash) -> anyhow::Result<PathBuf> {
        let hash: Id20 = match id {
            TorrentIdOrHash::Hash(hash) => hash,
            TorrentIdOrHash::Id(id) => bail!("torrent {id}: this store goes by info hash"),
        };
        Ok(self.folder.join(format!("{hash:?}.bitv")))
    }
}

#[async_trait::async_trait]
impl BitVFactory for FolderBitVFactory {
    async fn load(&self, id: TorrentIdOrHash) -> anyhow::Result<Option<Box<dyn BitV>>> {
        let filename = self.filename(id)?;
        match DiskBackedBitV::new(filename, self.spawner.clone()).await {
            Ok(bitv) => Ok(Some(bitv.into_dyn())),
            Err(e) => match e.downcast_ref::<std::io::Error>() {
                Some(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                _ => Err(e),
            },
        }
    }

    async fn clear(&self, id: TorrentIdOrHash) -> anyhow::Result<()> {
        let filename = self.filename(id)?;
        tokio::fs::remove_file(&filename)
            .await
            .with_context(|| format!("error removing {filename:?}"))
    }

    async fn store_initial_check(
        &self,
        id: TorrentIdOrHash,
        b: BF,
    ) -> anyhow::Result<Box<dyn BitV>> {
        let filename = self.filename(id)?;
        let tmp = filename.with_extension("bitv.tmp");
        tokio::fs::write(&tmp, b.as_raw_slice())
            .await
            .with_context(|| format!("error writing {tmp:?}"))?;
        tokio::fs::rename(&tmp, &filename)
            .await
            .with_context(|| format!("error renaming {tmp:?} to {filename:?}"))?;
        Ok(DiskBackedBitV::new(filename, self.spawner.clone())
            .await?
            .into_dyn())
    }
}

pub struct NonPersistentBitVFactory {}

#[async_trait::async_trait]
impl BitVFactory for NonPersistentBitVFactory {
    async fn load(&self, _: TorrentIdOrHash) -> anyhow::Result<Option<Box<dyn BitV>>> {
        Ok(None)
    }

    async fn clear(&self, _id: TorrentIdOrHash) -> anyhow::Result<()> {
        Ok(())
    }

    async fn store_initial_check(
        &self,
        _id: TorrentIdOrHash,
        b: BF,
    ) -> anyhow::Result<Box<dyn BitV>> {
        Ok(Box::new(b))
    }
}
