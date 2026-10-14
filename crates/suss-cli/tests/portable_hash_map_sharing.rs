//! Execute physical HAMT path sharing with canonical roots in both phases.
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;

use portable_decode::{Decoder, Observation};
use suss_cli::portable_session::{Session, SessionValue};
use wasmtime::{AnyRef, OwnedRooted, Rooted, StorageType, Val};

const FIXTURE: &str = include_str!("../../../tests/oracle/fixtures/hamt-sharing.sus");

struct Canonical {
    anchor: SessionValue,
    descriptor: OwnedRooted<AnyRef>,
    ty: wasmtime::StructType,
    fields: &'static [&'static str],
}

fn canonical(session: &mut Session, source: &str, fields: &'static [&'static str]) -> Canonical {
    let anchor = session.eval(source).unwrap_or_else(|e| panic!("{source}: {e:?}"));
    let (descriptor, ty) = session
        .inspect(&anchor, |mut store, value| {
            let owner = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let descriptor = owner
                .field(&mut store, 0)?
                .unwrap_anyref()
                .unwrap()
                .to_owned_rooted(&mut store)?;
            Ok((descriptor, owner.ty(&store)?))
        })
        .unwrap();
    Canonical { anchor, descriptor, ty, fields }
}

fn checked_fields(
    store: &mut wasmtime::StoreContextMut<'_, ()>,
    value: &Val,
    class: &Canonical,
) -> wasmtime::Result<Vec<Val>> {
    let owner = value.unwrap_anyref().unwrap().as_struct(&*store)?
        .ok_or_else(|| wasmtime::Error::msg("expected canonical HAMT owner"))?;
    if !wasmtime::StructType::eq(&owner.ty(&*store)?, &class.ty) {
        return Err(wasmtime::Error::msg("HAMT owner type differs from canonical root"));
    }
    let descriptor_value = owner.field(&mut *store, 0)?;
    let descriptor_ref = descriptor_value.unwrap_anyref().unwrap();
    if !Rooted::ref_eq(&*store, &class.descriptor, descriptor_ref)? {
        return Err(wasmtime::Error::msg("HAMT owner has noncanonical descriptor"));
    }
    let descriptor = descriptor_ref.as_struct(&*store)?.unwrap();
    if descriptor.fields(&mut *store)?.count() != 5 {
        return Err(wasmtime::Error::msg("malformed class descriptor"));
    }
    let schema_value = descriptor.field(&mut *store, 1)?;
    let schema = schema_value.unwrap_anyref().unwrap().as_array(&*store)?
        .ok_or_else(|| wasmtime::Error::msg("malformed class schema array"))?;
    let slots_value = owner.field(&mut *store, 1)?;
    let slots = slots_value.unwrap_anyref().unwrap().as_array(&*store)?
        .ok_or_else(|| wasmtime::Error::msg("malformed class slot array"))?;
    if schema.len(&*store)? != class.fields.len() as u32
        || slots.len(&*store)? != class.fields.len() as u32
    {
        return Err(wasmtime::Error::msg("canonical field count mismatch"));
    }
    let mut names = Vec::new();
    for index in 0..schema.len(&*store)? {
        let name = schema.get(&mut *store, index)?.unwrap_anyref().unwrap()
            .as_array(&*store)?
            .ok_or_else(|| wasmtime::Error::msg("malformed field name"))?;
        if name.len(&*store)? > 32
            || !matches!(name.ty(&*store)?.element_type(), StorageType::I16)
        {
            return Err(wasmtime::Error::msg("invalid bounded field name storage"));
        }
        let units = name.elems(&mut *store)?.map(|v| v.unwrap_i32() as u16).collect::<Vec<_>>();
        names.push(String::from_utf16(&units).map_err(|_| wasmtime::Error::msg("invalid field name UTF-16"))?);
    }
    if names.iter().map(String::as_str).collect::<Vec<_>>() != class.fields {
        return Err(wasmtime::Error::msg("canonical field schema mismatch"));
    }
    Ok(slots.elems(&mut *store)?.collect())
}

fn uint32_number(store: &mut wasmtime::StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<u32> {
    let number = value.unwrap_anyref().unwrap().as_struct(&*store)?
        .ok_or_else(|| wasmtime::Error::msg("expected boxed number"))?;
    if number.fields(&mut *store)?.count() != 1 {
        return Err(wasmtime::Error::msg("malformed boxed number"));
    }
    let Val::F64(bits) = number.field(&mut *store, 0)? else {
        return Err(wasmtime::Error::msg("malformed boxed number payload"));
    };
    let n = f64::from_bits(bits);
    if !n.is_finite() || n < 0.0 || n > u32::MAX as f64 || n.fract() != 0.0 {
        return Err(wasmtime::Error::msg("invalid bounded array length/index"));
    }
    Ok(n as u32)
}

fn is_nil(store: &mut wasmtime::StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<bool> {
    Ok(value.unwrap_anyref().unwrap().as_i31(&*store)?.is_some_and(|v| v.get_u32() == 0))
}

// Validate the private source-array owner and unsparse its exact backing.
// Missing dense slots remain absent; sparse entries are range/order checked.
fn source_array(
    store: &mut wasmtime::StoreContextMut<'_, ()>,
    value: &Val,
    source_array_class: &Canonical,
) -> wasmtime::Result<Vec<Val>> {
    let owner = value.unwrap_anyref().unwrap().as_struct(&*store)?
        .ok_or_else(|| wasmtime::Error::msg("expected source-array owner"))?;
    if !wasmtime::StructType::eq(&owner.ty(&*store)?, &source_array_class.ty)
        || owner.fields(&mut *store)?.count() != 4
    {
        return Err(wasmtime::Error::msg("invalid source-array owner layout"));
    }
    let descriptor_value = owner.field(&mut *store, 0)?;
    let descriptor = descriptor_value.unwrap_anyref().unwrap();
    if !Rooted::ref_eq(&*store, &source_array_class.descriptor, descriptor)? {
        return Err(wasmtime::Error::msg("noncanonical source-array descriptor"));
    }
    // SourceArray.field1 is a two-element wrapper [raw-backing, flag]. The
    // first item is the four-field backing; field1 must be the exact zero flag.
    let wrapper_value = owner.field(&mut *store, 1)?;
    let wrapper = wrapper_value.unwrap_anyref().unwrap().as_array(&*store)?
        .ok_or_else(|| wasmtime::Error::msg("invalid source-array wrapper"))?;
    if wrapper.len(&*store)? != 2
        || !matches!(wrapper.ty(&*store)?.element_type(), StorageType::ValType(wasmtime::ValType::Ref(_)))
    {
        return Err(wasmtime::Error::msg("invalid source-array wrapper layout"));
    }
    let wrapped = wrapper.elems(&mut *store)?.collect::<Vec<_>>();
    if wrapped[1].unwrap_anyref().unwrap().as_i31(&*store)?.map(|v| v.get_u32()) != Some(0) {
        return Err(wasmtime::Error::msg("invalid source-array wrapper flag"));
    }
    let backing = wrapped[0].unwrap_anyref().unwrap().as_array(&*store)?
        .ok_or_else(|| wasmtime::Error::msg("invalid source-array raw backing"))?;
    if backing.len(&*store)? != 4 {
        return Err(wasmtime::Error::msg("invalid source-array backing fields"));
    }
    let backing = backing.elems(&mut *store)?.collect::<Vec<_>>();
    let len = uint32_number(store, &backing[0])? as usize;
    if len > 4096 {
        return Err(wasmtime::Error::msg("scratch inspector source-array bound"));
    }
    let dense = backing[2].unwrap_anyref().unwrap().as_array(&*store)?
        .ok_or_else(|| wasmtime::Error::msg("invalid dense array"))?;
    let mask = backing[3].unwrap_anyref().unwrap().as_array(&*store)?
        .ok_or_else(|| wasmtime::Error::msg("invalid dense presence mask"))?;
    if dense.len(&*store)? != mask.len(&*store)?
        || dense.len(&*store)? > len as u32
        || !matches!(dense.ty(&*store)?.element_type(), StorageType::ValType(wasmtime::ValType::Ref(_)))
        || !matches!(mask.ty(&*store)?.element_type(), StorageType::I16)
    {
        return Err(wasmtime::Error::msg("invalid dense source-array storage"));
    }
    let undefined = Val::AnyRef(Some(AnyRef::from_i31(&mut *store, wasmtime::I31::wrapping_u32(6))));
    let mut result = vec![undefined; len];
    for index in 0..dense.len(&*store)? {
        match mask.get(&mut *store, index)?.unwrap_i32() {
            0 => (),
            1 => result[index as usize] = dense.get(&mut *store, index)?,
            _ => return Err(wasmtime::Error::msg("invalid source-array presence bit")),
        }
    }
    let mut next = backing[1].clone();
    let mut previous = None;
    let mut visited = Vec::<OwnedRooted<AnyRef>>::new();
    while !is_nil(store, &next)? {
        if visited.len() >= 4096 {
            return Err(wasmtime::Error::msg("source-array sparse-chain bound"));
        }
        let current = next.unwrap_anyref().unwrap();
        for prior in &visited {
            if Rooted::ref_eq(&*store, prior, current)? {
                return Err(wasmtime::Error::msg("cyclic source-array sparse chain"));
            }
        }
        visited.push(current.to_owned_rooted(&mut *store)?);
        let node = current.as_array(&*store)?.ok_or_else(|| wasmtime::Error::msg("invalid sparse node"))?;
        if node.len(&*store)? != 3 {
            return Err(wasmtime::Error::msg("invalid sparse node fields"));
        }
        let node = node.elems(&mut *store)?.collect::<Vec<_>>();
        let key = uint32_number(store, &node[0])?;
        if previous.is_some_and(|old| key <= old) || key as usize >= len {
            return Err(wasmtime::Error::msg("invalid sparse key order/range"));
        }
        if key < dense.len(&*store)? {
            return Err(wasmtime::Error::msg("dense/sparse overlap"));
        }
        result[key as usize] = node[1].clone();
        previous = Some(key);
        next = node[2].clone();
    }
    Ok(result)
}

fn map_root(
    session: &mut Session,
    map: &SessionValue,
    map_class: &Canonical,
    bitmap_class: &Canonical,
) -> OwnedRooted<AnyRef> {
    session.inspect(map, |mut store, value| {
        let fields = checked_fields(&mut store, &value, map_class)?;
        let root = fields[2].clone();
        let _ = checked_fields(&mut store, &root, bitmap_class)?;
        Ok(root.unwrap_anyref().unwrap().to_owned_rooted(&mut store)?)
    }).unwrap()
}

fn node_child(
    session: &mut Session,
    anchor: &SessionValue,
    node: &OwnedRooted<AnyRef>,
    bit: u32,
    bitmap_class: &Canonical,
    source_array_class: &Canonical,
) -> OwnedRooted<AnyRef> {
    session.inspect(anchor, |mut store, _| {
        let value = Val::AnyRef(Some(node.to_rooted(&mut store)));
        let fields = checked_fields(&mut store, &value, bitmap_class)?;
        let bitmap = uint32_number(&mut store, &fields[1])?;
        if bitmap & (1 << bit) == 0 {
            return Err(wasmtime::Error::msg("expected HAMT branch absent"));
        }
        let index = (bitmap & ((1 << bit) - 1)).count_ones() as usize;
        let slots = source_array(&mut store, &fields[2], source_array_class)?;
        let key = &slots[index * 2];
        if !is_nil(&mut store, key)? {
            return Err(wasmtime::Error::msg("expected nested HAMT child slot"));
        }
        let child = slots[index * 2 + 1].clone();
        let _ = checked_fields(&mut store, &child, bitmap_class)?;
        Ok(child.unwrap_anyref().unwrap().to_owned_rooted(&mut store)?)
    }).unwrap()
}

fn collision_child(
    session: &mut Session,
    anchor: &SessionValue,
    branch0: &OwnedRooted<AnyRef>,
    bitmap_class: &Canonical,
    collision_class: &Canonical,
    source_array_class: &Canonical,
) -> OwnedRooted<AnyRef> {
    session.inspect(anchor, |mut store, _| {
        let node = Val::AnyRef(Some(branch0.to_rooted(&mut store)));
        let fields = checked_fields(&mut store, &node, bitmap_class)?;
        let bitmap = uint32_number(&mut store, &fields[1])?;
        if bitmap & 1 == 0 { return Err(wasmtime::Error::msg("expected collision branch")); }
        let slots = source_array(&mut store, &fields[2], source_array_class)?;
        let index = (bitmap & 0).count_ones() as usize;
        if !is_nil(&mut store, &slots[index * 2])? {
            return Err(wasmtime::Error::msg("expected nested collision slot"));
        }
        let child = slots[index * 2 + 1].clone();
        let _ = checked_fields(&mut store, &child, collision_class)?;
        Ok(child.unwrap_anyref().unwrap().to_owned_rooted(&mut store)?)
    }).unwrap()
}

fn same(session: &mut Session, anchor: &SessionValue, a: &OwnedRooted<AnyRef>, b: &OwnedRooted<AnyRef>) -> bool {
    session.inspect(anchor, |store, _| Rooted::ref_eq(&store, a, b)).unwrap()
}

#[test]
fn persistent_hamt_update_copies_selected_path_and_retains_unrelated_child_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // Shipped bootstrap/default operation budget only: no direct core.sus
        // load and no test-specific fuel override.
        session.eval(FIXTURE).unwrap();
        let mut decoder = Decoder::capture(&mut session, 256).unwrap();
        // SourceArray is private and has no nominal field-name schema to copy:
        // capture its exact descriptor/type root, then validate its actual
        // four-field owner and two-slot wrapper in source_array().
        let source_array_class = canonical(&mut session, "(suss.core/array)", &[]);
        let map_class = canonical(&mut session, "share-old", &["meta", "cnt", "root", "has_nil_QMARK_", "nil_val", "__hash"]);
        let bitmap_class = canonical(&mut session, "(new suss.core/BitmapIndexedNode nil 0 (array))", &["edit", "bitmap", "arr"]);
        let collision_class = canonical(&mut session, "(new suss.core/HashCollisionNode nil 0 0 (array))", &["edit", "collision_hash", "cnt", "arr"]);
        let _anchors = (&source_array_class.anchor, &map_class.anchor, &bitmap_class.anchor, &collision_class.anchor);

        let old = session.eval("share-old").unwrap();
        let changed = session.eval("share-changed").unwrap();
        let other = session.eval("share-other-changed").unwrap();
        session.collect().unwrap();

        let old_root = map_root(&mut session, &old, &map_class, &bitmap_class);
        let changed_root = map_root(&mut session, &changed, &map_class, &bitmap_class);
        let other_root = map_root(&mut session, &other, &map_class, &bitmap_class);
        assert!(!same(&mut session, &old, &old_root, &changed_root), "changed map root is copied");
        assert!(!same(&mut session, &old, &old_root, &other_root), "negative-control map root is copied");

        let old_b0 = node_child(&mut session, &old, &old_root, 0, &bitmap_class, &source_array_class);
        let old_b1 = node_child(&mut session, &old, &old_root, 1, &bitmap_class, &source_array_class);
        let changed_b0 = node_child(&mut session, &changed, &changed_root, 0, &bitmap_class, &source_array_class);
        let changed_b1 = node_child(&mut session, &changed, &changed_root, 1, &bitmap_class, &source_array_class);
        let other_b0 = node_child(&mut session, &other, &other_root, 0, &bitmap_class, &source_array_class);
        let other_b1 = node_child(&mut session, &other, &other_root, 1, &bitmap_class, &source_array_class);
        let old_collision = collision_child(&mut session, &old, &old_b0, &bitmap_class, &collision_class, &source_array_class);
        let changed_collision = collision_child(&mut session, &changed, &changed_b0, &bitmap_class, &collision_class, &source_array_class);
        let other_collision = collision_child(&mut session, &other, &other_b0, &bitmap_class, &collision_class, &source_array_class);

        assert!(!same(&mut session, &old, &old_b0, &changed_b0), "selected branch 0 copied");
        assert!(same(&mut session, &old, &old_b1, &changed_b1), "untouched branch 1 shared");
        assert!(!same(&mut session, &old, &old_collision, &changed_collision), "selected collision path copied");

        assert!(same(&mut session, &old, &old_b0, &other_b0), "negative control retains branch 0");
        assert!(same(&mut session, &old, &old_collision, &other_collision), "negative control retains collision node");
        assert!(!same(&mut session, &old, &old_b1, &other_b1), "negative control copies selected branch 1");

        for (source, expected) in [
            ("(vector (get share-old share-k0) (get share-old share-k1) (get share-old share-k2) (get share-old share-k3) (get share-old share-k4))", [100.0_f64, 101.0, 102.0, 103.0, 104.0]),
            ("(vector (get share-changed share-k0) (get share-changed share-k1) (get share-changed share-k2) (get share-changed share-k3) (get share-changed share-k4))", [900.0_f64, 101.0, 102.0, 103.0, 104.0]),
            ("(vector (get share-other-changed share-k0) (get share-other-changed share-k1) (get share-other-changed share-k2) (get share-other-changed share-k3) (get share-other-changed share-k4))", [100.0_f64, 101.0, 102.0, 903.0, 104.0]),
        ] {
            // These lookups execute after both updates and the earlier GC;
            // fixture values are not cached snapshots.
            let observed = session.eval(source).unwrap();
            session.collect().unwrap();
            decoder.refill(256);
            let Observation::Vector(values) = decoder.decode_session(&mut session, &observed).unwrap() else {
                panic!("{source}: expected independently decoded vector")
            };
            assert_eq!(values.len(), expected.len(), "{source}");
            for (actual, expected) in values.iter().zip(expected) {
                let Observation::Number(actual_bits) = actual else {
                    panic!("{source}: unexpected decoded value {actual:?}")
                };
                assert_eq!(*actual_bits, expected.to_bits(), "{source}");
            }
        }
    }
}
