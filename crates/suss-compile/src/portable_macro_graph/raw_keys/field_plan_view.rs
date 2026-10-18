//! Test-only checked view of the SAME SFP1 BYTE artifact. No owned Field/Row or
//! child vectors, no owned decoder under the view meter, no runtime authority.
//! Offset storage and a single reusable DFS stack are prepaid. Native fault
//! surrogates below are not Session IRQ/fuel. Source binding/runtime installation
//! remain UNIMPLEMENTED; decode alone grants none. IDs require a fresh immutable
//! byte prefix in caller/source phase, never an already-populated Graph.
use super::*;
use std::ops::Range;
const NONE: usize = usize::MAX;

// View slicing does NOT copy/read the payload. Charge fixed checked range work;
// callers reading bytes (ULEBs, hashes, equality, UTF8) pay those reads separately.
struct Cursor<'a> { bytes: &'a [u8], at: usize }
impl<'a> Cursor<'a> {
    fn take(&mut self,n:usize,m:&mut Meter)->Result<&'a[u8]> {
        m.pay(2)?;let end=self.at.checked_add(n).ok_or(Failure::Overflow)?;
        let data=self.bytes.get(self.at..end).ok_or(Failure::Invalid("truncated"))?;
        self.at=end;Ok(data)
    }
    fn byte(&mut self,m:&mut Meter)->Result<u8> {let data=self.take(1,m)?;m.pay(1)?;Ok(data[0])}
    fn integer(&mut self,m:&mut Meter)->Result<usize> {
        let mut value=0u64;
        for shift in (0..70).step_by(7) {
            m.pay(6)?;let b=self.byte(m)?;
            if shift==63&&b&0xfe!=0{return Err(Failure::Overflow);}
            value|=u64::from(b&127)<<shift;
            if b&128==0 {
                if shift!=0&&b==0{return Err(Failure::Invalid("noncanonical integer"));}
                return usize::try_from(value).map_err(|_|Failure::Overflow);
            }
        }
        Err(Failure::Overflow)
    }
    fn extent(&self,n:usize,width:usize,m:&mut Meter)->Result<()> {
        m.pay(2)?;let needed=n.checked_mul(width).ok_or(Failure::Overflow)?;
        if needed>self.bytes.len()-self.at{return Err(Failure::Invalid("impossible count"));}Ok(())
    }
    fn end(&self,m:&mut Meter)->Result<()> {m.pay(1)?;if self.at!=self.bytes.len(){Err(Failure::Invalid("trailing bytes"))}else{Ok(())}}
}
// Offset/range reads are prepaid once when this iterator is minted; hashes and
// exact equality in its consumer still charge payload reads. No ref Vec collect.
pub(super) struct KeyViews<'a,'v> { bytes:&'a[u8], keys:std::slice::Iter<'v,KeyOffset> }
impl<'a> Iterator for KeyViews<'a,'_> {
    type Item=&'a[u8];
    fn next(&mut self)->Option<Self::Item> {self.keys.next().map(|k|&self.bytes[k.start..k.end])}
    fn size_hint(&self)->(usize,Option<usize>) {self.keys.size_hint()}
}
impl ExactSizeIterator for KeyViews<'_,'_> {}


#[derive(Clone, Copy)]
struct KeyOffset {
    start: usize, end: usize, metadata_at: usize, body_at: usize,
    metadata: usize, body: usize,
}
#[derive(Clone, Copy)]
struct OwnerOffset { start: usize, end: usize }
#[derive(Clone, Copy)]
struct FieldOffset { header: usize, row_base: usize, rows: usize, end: usize }
pub(super) struct FieldPlanView<'a> {
    bytes: &'a [u8],
    identity: Identity,
    keys: Vec<KeyOffset>,
    owners: Vec<OwnerOffset>,
    fields: Vec<FieldOffset>,
    rows: Vec<usize>,
}
struct OwnerView<'a> { ordinal: usize, namespace: &'a [u8], name: &'a [u8], mask: usize }
fn blob_range(r: &mut Cursor<'_>, m: &mut Meter) -> Result<Range<usize>> {
    let n = r.integer(m)?; let start = r.at; r.take(n, m)?; Ok(start..r.at)
}
fn owner<'a>(bytes: &'a [u8], o: OwnerOffset, m: &mut Meter) -> Result<OwnerView<'a>> {
    m.pay(2)?;
    let mut r = Cursor { bytes: bytes.get(o.start..o.end).ok_or(Failure::Invalid("owner offset"))?, at: 0 };
    let ordinal = r.integer(m)?;
    let ns = blob_range(&mut r, m)?; let name = blob_range(&mut r, m)?;
    let mask = r.integer(m)?; r.end(m)?;
    Ok(OwnerView { ordinal, namespace: &r.bytes[ns], name: &r.bytes[name], mask })
}
fn key_offset(bytes: &[u8], span: Range<usize>, id: usize, m: &mut Meter) -> Result<KeyOffset> {
    let mut r = Cursor { bytes: &bytes[span.clone()], at: 0 };
    let metadata = r.integer(m)?; r.extent(metadata, 1, m)?;
    let metadata_at = span.start.checked_add(r.at).ok_or(Failure::Overflow)?;
    for _ in 0..metadata { if r.integer(m)? >= id { return Err(Failure::Invalid("key metadata domain")); } }
    let tag = r.byte(m)?;
    let body = match tag {
        0 => 0,
        1 => { if r.byte(m)? > 1 { return Err(Failure::Invalid("boolean")); } 0 }
        2 => { r.take(8, m)?; 0 }
        3 => { let n=r.integer(m)?.checked_mul(2).ok_or(Failure::Overflow)?;r.take(n,m)?;0 }
        4 | 5 => {
            let ns=r.byte(m)?;if ns>1{return Err(Failure::Invalid("identifier"));}
            for _ in 0..1+usize::from(ns) {
                let n=r.integer(m)?;let text=r.take(n,m)?;m.pay(units(text.len())?)?;
                std::str::from_utf8(text).map_err(|_|Failure::Invalid("utf8"))?;
            }
            0
        }
        6..=10 | 13 => r.integer(m)?,
        11 => 1,
        12 => 2,
        _ => return Err(Failure::Invalid("key tag")),
    };
    r.extent(body,1,m)?;
    let body_at=span.start.checked_add(r.at).ok_or(Failure::Overflow)?;
    for _ in 0..body {if r.integer(m)? >= id{return Err(Failure::Invalid("key child domain"));}}
    r.end(m)?;
    Ok(KeyOffset { start:span.start,end:span.end,metadata_at,body_at,metadata,body })
}
impl<'a> FieldPlanView<'a> {
    pub(super) fn key_count(&self) -> usize { self.keys.len() }
    pub(super) fn keys<'v>(&'v self,m:&mut Meter)->Result<KeyViews<'a,'v>> {
        m.pay(units(self.keys.len())?.checked_mul(3).ok_or(Failure::Overflow)?)?;
        Ok(KeyViews{bytes:self.bytes,keys:self.keys.iter()})
    }

    pub(super) fn key_bytes(&self, index: usize, m: &mut Meter) -> Result<&'a [u8]> {
        m.pay(3)?;
        let key=self.keys.get(index).ok_or(Failure::Invalid("KeyId domain"))?;
        self.bytes.get(key.start..key.end).ok_or(Failure::Invalid("key offset"))
    }
    pub(super) fn field_count(&self) -> usize { self.fields.len() }
    pub(super) fn row_count(&self, field: usize, m: &mut Meter) -> Result<usize> {
        m.pay(2)?;Ok(self.fields.get(field).ok_or(Failure::Invalid("field index"))?.rows)
    }
    // Header contains owner, selector and root in the unchanged wire order.
    pub(super) fn field_header(&self, index: usize, m: &mut Meter) -> Result<(usize,usize,usize)> {
        m.pay(2)?;let f=self.fields.get(index).ok_or(Failure::Invalid("field index"))?;
        let mut r=Cursor{bytes:self.bytes.get(f.header..f.end).ok_or(Failure::Invalid("field offset"))?,at:0};
        Ok((r.integer(m)?,r.integer(m)?,r.integer(m)?))
    }
    fn row_reader(&self, field: usize, index: usize, m: &mut Meter) -> Result<Cursor<'a>> {
        m.pay(4)?;let f=self.fields.get(field).ok_or(Failure::Invalid("field index"))?;
        if index>=f.rows{return Err(Failure::Invalid("row index"));}
        let pos=f.row_base.checked_add(index).ok_or(Failure::Overflow)?;
        let start=*self.rows.get(pos).ok_or(Failure::Invalid("row offset"))?;
        Ok(Cursor{bytes:self.bytes.get(start..f.end).ok_or(Failure::Invalid("row offset"))?,at:0})
    }
    pub(super) fn row_header(&self, field:usize,index:usize,m:&mut Meter)->Result<(usize,usize,usize)> {
        let mut r=self.row_reader(field,index,m)?;Ok((r.integer(m)?,r.integer(m)?,r.integer(m)?))
    }
    // Reads are checked ULEBs, not an owned child collection. Caller can stream
    // the children once via visit_children; indexed access is explicitly O(i).
    pub(super) fn visit_children(&self,field:usize,index:usize,m:&mut Meter,
        mut visit:impl FnMut(usize,&mut Meter)->Result<()>)->Result<()> {
        let mut r=self.row_reader(field,index,m)?;r.integer(m)?;r.integer(m)?;
        let n=r.integer(m)?;for _ in 0..n {let child=r.integer(m)?;visit(child,m)?;}Ok(())
    }
    pub(super) fn child(&self,field:usize,index:usize,child:usize,m:&mut Meter)->Result<usize> {
        let mut r=self.row_reader(field,index,m)?;r.integer(m)?;r.integer(m)?;
        let n=r.integer(m)?;if child>=n{return Err(Failure::Invalid("child ordinal"));}
        for i in 0..n {let value=r.integer(m)?;if i==child{return Ok(value);}}
        unreachable!("checked child ordinal")
    }
    pub(super) fn checked(bytes:&'a [u8],expected:Identity,m:&mut Meter)->Result<Self> {
        Self::checked_buckets(bytes,expected,m,false)
    }
    fn checked_buckets(bytes:&'a [u8],expected:Identity,m:&mut Meter,force_collision:bool)->Result<Self> {
        let mut r=Cursor{bytes,at:0};
        if !equal(r.take(4,m)?,b"SFP1",m)?||r.integer(m)?!=1{return Err(Failure::Invalid("version"));}
        let phase=match r.integer(m)?{0=>Phase::Runtime,1=>Phase::Macro,_=>return Err(Failure::Invalid("phase"))};
        if phase!=expected.phase{return Err(Failure::Invalid("phase"));}
        for pin in [&expected.source,&expected.compiler,&expected.dependency] {
            if !equal(r.take(32,m)?,pin,m)?{return Err(Failure::Invalid("provenance"));}
        }
        let count=r.integer(m)?;r.extent(count,1,m)?;
        if count==0{return Err(Failure::Invalid("empty catalog"));}
        let mut keys=Vec::new();room(&mut keys,count,m)?;
        for id in 0..count {let span=blob_range(&mut r,m)?;let k=key_offset(bytes,span,id,m)?;push(&mut keys,k,m)?;}
        let mut view=Self{bytes,identity:expected,keys,owners:Vec::new(),fields:Vec::new(),rows:Vec::new()};
        // Native offset-only chaining: hashes choose buckets, exact original
        // bytes reject duplicates. No copied keys/original Interner rebuild.
        let buckets=count.checked_mul(2).and_then(usize::checked_next_power_of_two).ok_or(Failure::Overflow)?;
        let mut heads=Vec::new();room(&mut heads,buckets,m)?;m.pay(units(buckets)?)?;heads.resize(buckets,NONE);
        let mut next=Vec::new();room(&mut next,count,m)?;
        for id in 0..count {
            let key=view.key_bytes(id,m)?;
            let hash=if force_collision{m.pay(1)?;0}else{super::super::hash_bytes(key,&mut |n|m.pay(n))?};
            m.pay(3)?;let bucket=(hash as usize)&(buckets-1);let mut cursor=heads[bucket];
            while cursor!=NONE {
                m.pay(2)?;
                if equal(key,view.key_bytes(cursor,m)?,m)?{return Err(Failure::Invalid("duplicate key ID"));}
                cursor=next[cursor];
            }
            let previous=heads[bucket];push(&mut next,previous,m)?;m.pay(1)?;heads[bucket]=id;
        }
        // Prepay normal index-vector release. Error cleanup remains the native
        // early-Drop policy, not a claim of accepted-charge-only total cleanup.
        m.pay(units(next.len())?.checked_add(units(heads.len())?).ok_or(Failure::Overflow)?)?;
        drop(next);drop(heads);
        let owners=r.integer(m)?;r.extent(owners,4,m)?;if owners==0{return Err(Failure::Invalid("empty catalog"));}
        room(&mut view.owners,owners,m)?;let mut expected_fields=0usize;
        for i in 0..owners {
            let start=r.at;r.integer(m)?;blob_range(&mut r,m)?;blob_range(&mut r,m)?;r.integer(m)?;
            let offset=OwnerOffset{start,end:r.at};let o=owner(bytes,offset,m)?;
            if o.mask!=3&&o.mask!=7{return Err(Failure::Invalid("presence"));}
            if !equal(o.namespace,b"suss.core",m)?||o.name.is_empty(){return Err(Failure::Invalid("Global"));}
            m.pay(units(o.name.len())?)?;std::str::from_utf8(o.name).map_err(|_|Failure::Invalid("global utf8"))?;
            if i!=0 {
                let old=owner(bytes,view.owners[i-1],m)?;
                if old.ordinal>=o.ordinal||super::super::compare_bytes(old.name,o.name,&mut |n|m.pay(n))?!=std::cmp::Ordering::Less{return Err(Failure::Invalid("owner order/duplicate"));}
            }
            expected_fields=expected_fields.checked_add(if o.mask==7{3}else{2}).ok_or(Failure::Overflow)?;
            push(&mut view.owners,offset,m)?;
        }
        let fields=r.integer(m)?;r.extent(fields,4,m)?;
        if fields!=expected_fields{return Err(Failure::Invalid("missing/extra field"));}
        room(&mut view.fields,fields,m)?;let mut scratch=Vec::new();
        for owner_id in 0..owners {
            let desc=owner(bytes,view.owners[owner_id],m)?;
            for selector in Selector::ALL {
                m.pay(2)?;if desc.mask&(1<<selector.index())==0{continue;}
                let header=r.at;
                if r.integer(m)?!=owner_id||Selector::from(r.integer(m)?)?!=selector{return Err(Failure::Invalid("field order/duplicate"));}
                if r.integer(m)?!=0{return Err(Failure::Invalid("root"));}
                let nodes=r.integer(m)?;r.extent(nodes,3,m)?;if nodes==0{return Err(Failure::Invalid("root"));}
                let row_base=view.rows.len();room(&mut view.rows,nodes,m)?;
                for _ in 0..nodes {
                    push(&mut view.rows,r.at,m)?;
                    let key=r.integer(m)?;let metadata=r.integer(m)?;let children=r.integer(m)?;
                    r.extent(children,1,m)?;
                    let shape=view.keys.get(key).ok_or(Failure::Invalid("KeyId domain"))?;
                    m.pay(3)?;
                    if metadata!=shape.metadata||children!=shape.metadata.checked_add(shape.body).ok_or(Failure::Overflow)?{return Err(Failure::Invalid("metadata/edge count"));}
                    for _ in 0..children{r.integer(m)?;}
                }
                let field=view.fields.len();push(&mut view.fields,FieldOffset{header,row_base,rows:nodes,end:r.at},m)?;
                view.validate_field(field,&mut scratch,m)?;
            }
        }
        r.end(m)?;m.pay(1)?;Ok(view)
    }
    fn validate_field(&self,field:usize,scratch:&mut Vec<usize>,m:&mut Meter)->Result<()> {
        // One invocation-local reusable buffer; no per-field child arrays. DFS
        // preorder == 0..N plus forward child indexes proves closed occurrence,
        // exactly one incoming edge, no cycle, no sharing/disconnected extras.
        m.pay(1)?;if !scratch.is_empty(){return Err(Failure::Invalid("scratch state"));}
        push(scratch,0,m)?;let mut expected=0;
        while let Some(index)=scratch.pop() {
            m.pay(4)?;if index!=expected{return Err(Failure::Invalid("cycle/disconnected/duplicate occurrence"));}
            expected=expected.checked_add(1).ok_or(Failure::Overflow)?;
            let mut row=self.row_reader(field,index,m)?;
            let key=row.integer(m)?;let metadata=row.integer(m)?;let children=row.integer(m)?;
            let shape=self.keys.get(key).ok_or(Failure::Invalid("KeyId domain"))?;
            if metadata!=shape.metadata||children!=shape.metadata.checked_add(shape.body).ok_or(Failure::Overflow)?{return Err(Failure::Invalid("metadata/edge count"));}
            let mut meta=Cursor{bytes:&self.bytes[..shape.end],at:shape.metadata_at};
            let mut body=Cursor{bytes:&self.bytes[..shape.end],at:shape.body_at};
            let start=scratch.len();
            for ordinal in 0..children {
                let child=row.integer(m)?;
                if child<=index{return Err(Failure::Invalid("child index/key domain"));}
                let canonical=if ordinal<metadata{meta.integer(m)?}else{body.integer(m)?};
                let mut child_row=self.row_reader(field,child,m)?;
                if child_row.integer(m)?!=canonical{return Err(Failure::Invalid("child index/key domain"));}
                push(scratch,child,m)?;
            }
            // Reverse ONLY this node's newly appended children, preserving all
            // pending sibling order; prepay each bounded pair read/write.
            let n=scratch.len()-start;
            for i in 0..n/2 {m.pay(4)?;scratch.swap(start+i,start+n-1-i);}
        }
        if expected!=self.fields[field].rows{return Err(Failure::Invalid("disconnected rows"));}
        m.pay(0)
    }
    pub(super) fn clear_offsets(&mut self,m:&mut Meter)->Result<()> {
        for len in [self.keys.len(),self.owners.len(),self.fields.len(),self.rows.len()] {
            m.pay(units(len)?)?;
        }
        self.keys.clear();self.owners.clear();self.fields.clear();self.rows.clear();Ok(())
    }
}

#[test]
fn streamed_view_all_catalog_fields_match_original_and_owned_with_separate_startup_costs() {
    use super::super::native_prefix::{BorrowedInterner,BorrowedPrefix};
    for phase in [Phase::Runtime,Phase::Macro] {
        let prepared=bootstrap::shipped(phase).unwrap();
        let mut setup=Meter::default();let id=identity(phase,&mut setup).unwrap();
        let c=catalog(phase,&prepared.environment,&mut setup).unwrap();assert_eq!(c.owners.len(),615);
        let mut original_work=Meter::default();let mut snapshot=Meter::default();
        let (oracle,mut original_keys,mut original_plan)=original(&c,id,&mut original_work,&mut snapshot).unwrap();
        assert_eq!(oracle.fields.len(),1577);
        let mut warm=Meter::default();let mut controls=Meter::default();
        warm_original(&c,&oracle,&mut original_keys,&mut original_plan,&mut warm,&mut controls).unwrap();
        let mut encoder=Meter::default();let bytes=encode(&oracle,&mut encoder).unwrap();
        // Owned decode is a retained negative control, OUTSIDE view timing.
        let mut owned_work=Meter::default();let owned=decode(&bytes,id,&mut owned_work).unwrap();
        compare_artifacts(&oracle,&owned,&mut controls).unwrap();
        let mut view_work=Meter::default();let mut view=FieldPlanView::checked(&bytes,id,&mut view_work).unwrap();
        assert_eq!(view.identity,id);assert_eq!(view.key_count(),owned.keys.len());assert_eq!(view.field_count(),1577);
        for (key,expected) in owned.keys.iter().enumerate() {
            let actual=view.key_bytes(key,&mut controls).unwrap();assert!(equal(actual,expected,&mut controls).unwrap());
            let start=actual.as_ptr() as usize;let origin=bytes.as_ptr() as usize;
            assert!(start>=origin&&start+actual.len()<=origin+bytes.len(),"borrow exact artifact storage");
        }
        for (index,field) in owned.fields.iter().enumerate() {
            assert_eq!(view.field_header(index,&mut controls).unwrap(),(field.owner,field.selector.index(),field.root));
            assert_eq!(view.row_count(index,&mut controls).unwrap(),field.rows.len());
            for (node,row) in field.rows.iter().enumerate() {
                assert_eq!(view.row_header(index,node,&mut controls).unwrap(),(row.key,row.metadata,row.children.len()));
                let mut ordinal=0;
                view.visit_children(index,node,&mut controls,|child,m| {
                    m.pay(1)?;assert_eq!(child,row.children[ordinal]);ordinal+=1;Ok(())
                }).unwrap();assert_eq!(ordinal,row.children.len());
            }
        }
        // Startup builder borrows exact validated key slices in canonical ID
        // order. This is a FRESH native prefix, never a live Graph install.
        let mut startup=Meter::default();
        let key_views=view.keys(&mut startup).unwrap();
        let prefix=BorrowedPrefix::build(key_views,&mut |n|startup.pay(n)).unwrap();
        let mut attach=Meter::default();let mut interner=BorrowedInterner::new(&prefix,&mut |n|attach.pay(n)).unwrap();
        let mut replan_work=Meter::default();let mut scratch=Plan::default();let mut field=0;
        for n in &c.owners {
            for selector in Selector::ALL {
                controls.pay(1).unwrap();
                if let Some(form)=selector.field(n.owner) {
                    let root=interner.plan(form,&mut scratch,&mut |n|replan_work.pay(n)).unwrap();
                    assert_eq!(root,owned.fields[field].root);
                    compare_rows(&owned.fields[field].rows,&scratch,&mut controls).unwrap();
                    scratch.clear_metered(&mut |n|replan_work.pay(n)).unwrap();field+=1;
                }
            }
        }
        assert_eq!(field,1577);
        // Prefix retains artifact byte borrows, not offset Vec borrows. Release
        // interner/prefix before explicit offset cleanup; source bytes stay alive.
        drop(interner);drop(prefix);
        let mut cleanup=Meter::default();view.clear_offsets(&mut cleanup).unwrap();
        println!("field-plan-view phase={phase:?} owners=615 fields=1577 artifact_bytes={} original_cold_plan_clear={} original_warm_plan_clear={} encoder={} original_snapshot={} owned_decode_negative_control={} view_validate_and_offsets={} view_backing_subset={} prefix_BUILD_STARTUP={} perGraph_ATTACH={} borrowed_interner_STILL_REPLANS_ALL_FIELDS={} offset_cleanup={} control_comparisons={} source_identity_setup={} source_binding=UNIMPLEMENTED correspondence=UNIMPLEMENTED runtime_admission=UNIMPLEMENTED graph=UNCHANGED runtime_win=UNKNOWN",bytes.len(),original_work.work,warm.work,encoder.work,snapshot.work,owned_work.work,view_work.work,view_work.storage,startup.work,attach.work,replan_work.work,cleanup.work,controls.work,setup.work);
    }
}
fn bounded_fixture()->(Identity,Artifact,Vec<u8>) {
    let env=&bootstrap::shipped(Phase::Runtime).unwrap().environment;
    let c=catalog(Phase::Runtime,env,&mut Meter::default()).unwrap();let c=bounded(&c);
    let id=identity(c.phase,&mut Meter::default()).unwrap();let a=fixture(&c,id);
    let bytes=encode(&a,&mut Meter::default()).unwrap();(id,a,bytes)
}
fn reject(bytes:&[u8],id:Identity) {
    assert!(decode(bytes,id,&mut Meter::default()).is_err(),"owned control must reject same negative");
    assert!(FieldPlanView::checked(bytes,id,&mut Meter::default()).is_err(),"streamed view must reject same negative");
}
#[test]
fn streamed_view_rejects_owned_negative_corpus_and_forced_collision_duplicates() {
    let (id,a,bytes)=bounded_fixture();
    for n in 0..bytes.len(){reject(&bytes[..n],id);}
    let mut bad=bytes.clone();bad.push(0);reject(&bad,id);
    for offset in [0,4,5,6,38,70]{let mut b=bytes.clone();b[offset]^=1;reject(&b,id);}
    let mut bad=bytes[..102].to_vec();bad.extend_from_slice(&[255;10]);bad.push(0);reject(&bad,id);
    let mut variants=Vec::new();
    let mut b=a.clone();b.fields.pop();variants.push(b);
    let mut b=a.clone();b.fields.insert(0,b.fields[0].clone());variants.push(b);
    let mut b=a.clone();b.fields[0].owner=usize::MAX;variants.push(b);
    let mut b=a.clone();b.fields[0].root=usize::MAX;variants.push(b);
    let mut b=a.clone();b.fields[0].rows[0].key=usize::MAX;variants.push(b);
    let mut b=a.clone();b.fields[0].rows[0].metadata=usize::MAX;variants.push(b);
    let mut b=a.clone();b.fields[0].rows[0].children[0]=0;variants.push(b);
    let mut b=a.clone();b.fields[0].rows[0].children[0]=usize::MAX;variants.push(b);
    let mut b=a.clone();let extra=b.fields[0].rows[0].clone();b.fields[0].rows.push(extra);variants.push(b);
    let mut b=a.clone();let duplicate=b.fields[0].rows[0].children[0];b.fields[0].rows[0].children[1]=duplicate;variants.push(b);
    let mut b=a.clone();b.owners[0].mask=0;variants.push(b);
    let mut b=a.clone();b.keys.push(b.keys[0].clone());variants.push(b);
    let mut b=a.clone();b.keys[0].push(0);variants.push(b);
    let mut b=a.clone();b.keys[0]=vec![0,255];variants.push(b);
    let mut b=a.clone();b.owners.push(b.owners[0].clone());variants.push(b);
    for (i,b) in variants.iter().enumerate(){let encoded=encode(b,&mut Meter::default()).unwrap();reject(&encoded,id);println!("field-plan-view-negative case={i}");}
    // Same invalid-selector framing as the existing owned negative control.
    let mut m=Meter::default();let mut r=Cursor{bytes:&bytes,at:102};
    for _ in 0..r.integer(&mut m).unwrap(){blob_range(&mut r,&mut m).unwrap();}
    for _ in 0..r.integer(&mut m).unwrap(){r.integer(&mut m).unwrap();blob_range(&mut r,&mut m).unwrap();blob_range(&mut r,&mut m).unwrap();r.integer(&mut m).unwrap();}
    r.integer(&mut m).unwrap();r.integer(&mut m).unwrap();let selector=r.at;
    let mut b=bytes.clone();b[selector]=3;reject(&b,id);
    let normal=FieldPlanView::checked(&bytes,id,&mut Meter::default()).unwrap();
    let forced=FieldPlanView::checked_buckets(&bytes,id,&mut Meter::default(),true).unwrap();
    assert_eq!(normal.key_count(),forced.key_count());
    for index in 0..normal.key_count(){assert!(equal(normal.key_bytes(index,&mut Meter::default()).unwrap(),forced.key_bytes(index,&mut Meter::default()).unwrap(),&mut Meter::default()).unwrap());}
    let mut duplicate=a.clone();duplicate.keys.push(duplicate.keys[0].clone());let encoded=encode(&duplicate,&mut Meter::default()).unwrap();
    assert!(FieldPlanView::checked_buckets(&encoded,id,&mut Meter::default(),true).is_err());
}
#[test]
fn streamed_view_native_fault_cuts_retry_and_checked_access_domains() {
    let (id,_,bytes)=bounded_fixture();
    fault_cuts("streamed-view-only-native-surrogates",|m|FieldPlanView::checked(&bytes,id,m));
    let view=FieldPlanView::checked(&bytes,id,&mut Meter::default()).unwrap();
    assert!(view.key_bytes(usize::MAX,&mut Meter::default()).is_err());
    assert!(view.field_header(usize::MAX,&mut Meter::default()).is_err());
    assert!(view.row_header(0,usize::MAX,&mut Meter::default()).is_err());
    assert!(view.child(0,0,usize::MAX,&mut Meter::default()).is_err());
    // Per-access failure leaves validated bytes/offsets unchanged; exact retry.
    fault_cuts("streamed-key-access",|m|view.key_bytes(0,m));
    fault_cuts("streamed-row-access",|m|view.row_header(0,0,m));
    let (key,metadata,children)=view.row_header(0,0,&mut Meter::default()).unwrap();
    assert!(key<view.key_count());assert!(metadata<=children);
}
