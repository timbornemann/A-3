use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_TEMP_DIRECTORY_ATTEMPTS: u64 = 100;

static NEXT_TEMP_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[allow(dead_code)]
pub(crate) fn module_projection(
    graph: &LinkedGraph,
    ranking: &RankProjection,
    manifests: &[FileRevision],
) -> Result<ModuleProjection, Box<dyn Error>> {
    let policy = ModulePolicyVersion::v1();
    let module_id = ModuleId::from_bytes([202; 32]);
    let kind = if manifests.is_empty() {
        ModuleKind::PathBoundary
    } else {
        ModuleKind::ManifestBoundary
    };
    let memberships = graph
        .symbols()
        .iter()
        .map(|symbol| {
            let evidence = manifests.first().map_or_else(
                || ModuleMembershipEvidence::path(symbol.revision().clone()),
                |manifest| {
                    ModuleMembershipEvidence::manifest(symbol.revision().clone(), manifest.clone())
                },
            );
            ModuleMembership::new(module_id, symbol.id(), evidence)
        })
        .collect::<Vec<_>>();
    let ranked = ranking
        .symbols()
        .iter()
        .map(|rank| rank.symbol_id())
        .collect::<Vec<_>>();
    let central_truncated = ranked.len() > 16;
    let central = ranked.iter().take(16).copied().collect::<Vec<_>>();
    let entrypoints = ranked
        .iter()
        .copied()
        .filter(|id| has_role(graph, *id, SymbolRole::Entrypoint))
        .collect::<Vec<_>>();
    let tests = ranked
        .iter()
        .copied()
        .filter(|id| has_role(graph, *id, SymbolRole::Test))
        .collect::<Vec<_>>();
    let module = RepositoryModule::new(
        module_id,
        kind,
        Some(ModuleRoot::Repository),
        manifests.to_vec(),
        ModuleSymbolSet::new(central, central_truncated)?,
        ModuleSymbolSet::new(entrypoints.clone(), false)?,
        ModuleSymbolSet::new(tests, false)?,
    )?;
    let card = RepositoryCard::new(
        graph.snapshot_id(),
        policy,
        vec![module_id],
        vec![IndexLanguage::Generic],
        ModuleSymbolSet::new(entrypoints, false)?,
        u32::try_from(graph.files().len())?,
        u32::try_from(graph.symbols().len())?,
    )?;
    Ok(ModuleProjection::new(
        graph.snapshot_id(),
        policy,
        vec![module],
        memberships,
        card,
    )?)
}

#[allow(dead_code)]
fn has_role(graph: &LinkedGraph, id: a3_domain::SymbolId, role: SymbolRole) -> bool {
    graph
        .symbols()
        .binary_search_by_key(&id, |symbol| symbol.id())
        .ok()
        .and_then(|position| graph.symbols().get(position))
        .is_some_and(|symbol| symbol.parsed().roles().contains(role))
}

pub(crate) struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    pub(crate) fn new() -> io::Result<Self> {
        let base = std::env::temp_dir();
        for _ in 0..MAX_TEMP_DIRECTORY_ATTEMPTS {
            let sequence = NEXT_TEMP_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = base.join(format!("a3-storage-test-{}-{sequence}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not reserve a unique temporary storage test directory",
        ))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.path) {
            eprintln!(
                "could not remove temporary storage test directory {}: {error}",
                self.path.display()
            );
        }
    }
}
use a3_domain::{
    FileRevision, IndexLanguage, LinkedGraph, ModuleId, ModuleKind, ModuleMembership,
    ModuleMembershipEvidence, ModulePolicyVersion, ModuleProjection, ModuleRoot, ModuleSymbolSet,
    RankProjection, RepositoryCard, RepositoryModule, SymbolRole,
};
use std::error::Error;
