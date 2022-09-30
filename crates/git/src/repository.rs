use anyhow::Result;
use collections::HashMap;
use git2::Repository as LibGitRepository;
use parking_lot::Mutex;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use util::ResultExt;

#[async_trait::async_trait]
pub trait GitRepository: Send + Sync + std::fmt::Debug {
    fn manages(&self, path: &Path) -> bool;

    fn in_dot_git(&self, path: &Path) -> bool;

    fn content_path(&self) -> &Path;

    fn git_dir_path(&self) -> &Path;

    fn scan_id(&self) -> usize;

    fn set_scan_id(&mut self, scan_id: usize);

    fn reopen_git_repo(&mut self) -> bool;

    fn git_repo(&self) -> Arc<Mutex<LibGitRepository>>;

    fn boxed_clone(&self) -> Box<dyn GitRepository>;

    async fn load_head_text(&self, relative_file_path: &Path) -> Option<String>;
}

#[derive(Clone)]
pub struct RealGitRepository {
    // Path to folder containing the .git file or directory
    content_path: Arc<Path>,
    // Path to the actual .git folder.
    // Note: if .git is a file, this points to the folder indicated by the .git file
    git_dir_path: Arc<Path>,
    scan_id: usize,
    libgit_repository: Arc<Mutex<LibGitRepository>>,
}

impl RealGitRepository {
    pub fn open(dotgit_path: &Path) -> Option<Box<dyn GitRepository>> {
        LibGitRepository::open(&dotgit_path)
            .log_err()
            .and_then::<Box<dyn GitRepository>, _>(|libgit_repository| {
                Some(Box::new(Self {
                    content_path: libgit_repository.workdir()?.into(),
                    git_dir_path: dotgit_path.canonicalize().log_err()?.into(),
                    scan_id: 0,
                    libgit_repository: Arc::new(parking_lot::Mutex::new(libgit_repository)),
                }))
            })
    }
}

#[async_trait::async_trait]
impl GitRepository for RealGitRepository {
    fn manages(&self, path: &Path) -> bool {
        path.canonicalize()
            .map(|path| path.starts_with(&self.content_path))
            .unwrap_or(false)
    }

    fn in_dot_git(&self, path: &Path) -> bool {
        path.canonicalize()
            .map(|path| path.starts_with(&self.git_dir_path))
            .unwrap_or(false)
    }

    fn content_path(&self) -> &Path {
        self.content_path.as_ref()
    }

    fn git_dir_path(&self) -> &Path {
        self.git_dir_path.as_ref()
    }

    fn scan_id(&self) -> usize {
        self.scan_id
    }

    async fn load_head_text(&self, relative_file_path: &Path) -> Option<String> {
        fn logic(repo: &LibGitRepository, relative_file_path: &Path) -> Result<Option<String>> {
            const STAGE_NORMAL: i32 = 0;
            let index = repo.index()?;
            let oid = match index.get_path(relative_file_path, STAGE_NORMAL) {
                Some(entry) => entry.id,
                None => return Ok(None),
            };

            let content = repo.find_blob(oid)?.content().to_owned();
            let head_text = String::from_utf8(content)?;
            Ok(Some(head_text))
        }

        match logic(&self.libgit_repository.as_ref().lock(), relative_file_path) {
            Ok(value) => return value,
            Err(err) => log::error!("Error loading head text: {:?}", err),
        }
        None
    }

    fn reopen_git_repo(&mut self) -> bool {
        match LibGitRepository::open(&self.git_dir_path) {
            Ok(repo) => {
                self.libgit_repository = Arc::new(Mutex::new(repo));
                true
            }

            Err(_) => false,
        }
    }

    fn git_repo(&self) -> Arc<Mutex<LibGitRepository>> {
        self.libgit_repository.clone()
    }

    fn set_scan_id(&mut self, scan_id: usize) {
        self.scan_id = scan_id;
    }

    fn boxed_clone(&self) -> Box<dyn GitRepository> {
        Box::new(self.clone())
    }
}

impl std::fmt::Debug for RealGitRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitRepository")
            .field("content_path", &self.content_path)
            .field("git_dir_path", &self.git_dir_path)
            .field("scan_id", &self.scan_id)
            .field("libgit_repository", &"LibGitRepository")
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct FakeGitRepository {
    content_path: Arc<Path>,
    git_dir_path: Arc<Path>,
    scan_id: usize,
    state: Arc<Mutex<FakeGitRepositoryState>>,
}

#[derive(Debug, Clone, Default)]
pub struct FakeGitRepositoryState {
    pub index_contents: HashMap<PathBuf, String>,
}

impl FakeGitRepository {
    pub fn open(
        dotgit_path: &Path,
        scan_id: usize,
        state: Arc<Mutex<FakeGitRepositoryState>>,
    ) -> Box<dyn GitRepository> {
        Box::new(FakeGitRepository {
            content_path: dotgit_path.parent().unwrap().into(),
            git_dir_path: dotgit_path.into(),
            scan_id,
            state,
        })
    }
}

#[async_trait::async_trait]
impl GitRepository for FakeGitRepository {
    fn manages(&self, path: &Path) -> bool {
        path.starts_with(self.content_path())
    }

    fn in_dot_git(&self, path: &Path) -> bool {
        path.starts_with(self.git_dir_path())
    }

    fn content_path(&self) -> &Path {
        &self.content_path
    }

    fn git_dir_path(&self) -> &Path {
        &self.git_dir_path
    }

    fn scan_id(&self) -> usize {
        self.scan_id
    }

    async fn load_head_text(&self, path: &Path) -> Option<String> {
        let state = self.state.lock();
        state.index_contents.get(path).cloned()
    }

    fn reopen_git_repo(&mut self) -> bool {
        true
    }

    fn git_repo(&self) -> Arc<Mutex<LibGitRepository>> {
        unimplemented!()
    }

    fn set_scan_id(&mut self, scan_id: usize) {
        self.scan_id = scan_id;
    }

    fn boxed_clone(&self) -> Box<dyn GitRepository> {
        Box::new(self.clone())
    }
}
