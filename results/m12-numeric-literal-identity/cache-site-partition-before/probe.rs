use suss_compile::portable::{self, artifact_cache::ArtifactCache, hir::Literal, ir, resolve::Phase, SourceOrigin};
fn main() {
    let source="(do 1N 1N) ; original-site alias partition";
    let distinct=ir::lower(&portable::analyze(source).unwrap()).unwrap();
    let mut shared=distinct.clone();
    let mut sites=shared.blocks.iter_mut().flat_map(|block| &mut block.instructions)
        .filter(|instruction|matches!(instruction.operation,ir::Operation::Literal(Literal::BigInt{..})));
    let first=sites.next().expect("first source literal").literal_site.clone();
    assert!(first.is_some());
    sites.next().expect("second source literal").literal_site=first;
    assert!(sites.next().is_none());
    let origin=SourceOrigin::new(source,None);
    let emit=|cache:&mut ArtifactCache,graph:&ir::Function,deps|cache.emit(graph,Phase::Runtime,&[],Some(&origin),deps).unwrap();
    let fresh_distinct=emit(&mut ArtifactCache::default(),&distinct,None);
    let fresh_shared=emit(&mut ArtifactCache::default(),&shared,None);
    assert!(fresh_distinct!=fresh_shared,"fresh emitted artifacts must distinguish original literal-site aliasing");
    println!("Fresh artifacts differ: distinct={} bytes, shared={} bytes",fresh_distinct.len(),fresh_shared.len());
    let mut cache=ArtifactCache::default();
    assert!(emit(&mut cache,&distinct,Some(&[]))==fresh_distinct);
    let cached_shared=emit(&mut cache,&shared,Some(&[]));
    println!("After requesting both original-site alias partitions: {:?}",cache.stats());
    assert!(cached_shared==fresh_shared,"source artifact cache returned distinct-site artifact for shared original literal sites");
}
