from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
f=ROOT/'crates/suss-compile/tests/support/portable_decode.rs';s=f.read_text()
s=s.replace('    HashSet,\n','    HashSet,\n    TreeMap,\n    TreeSet,\n',1)
nodes='''            (
                "(new suss.core/BlackNode nil nil nil nil nil)",
                Class::BlackNode,
            ),
            (
                "(new suss.core/RedNode nil nil nil nil nil)",
                Class::RedNode,
            ),
'''
assert s.count(nodes)==1;s=s.replace(nodes,'')
needle='''            (
                "(new suss.core/BitmapIndexedNode nil 0 (suss.core/array))",
'''
assert s.count(needle)==1
anchors='''            (
                "(new suss.core/PersistentTreeMap nil nil 0 nil nil)",
                Class::TreeMap,
            ),
            (
                "(new suss.core/PersistentTreeSet nil (new suss.core/PersistentTreeMap nil nil 0 nil nil) nil)",
                Class::TreeSet,
            ),
'''
s=s.replace(needle,anchors+nodes+needle)
s=s.replace('                    Class::ArrayMap | Class::HashMap => {','                    Class::ArrayMap | Class::HashMap | Class::TreeMap => {',1)
a=s.index('                    Class::HashSet => {');b=s.index('                    Class::Vector =>',a)
s=s[:a]+'''                    Class::HashSet | Class::TreeSet => {
                        let (map_class, map_data) = self.object(store, &args[1])?;
                        let valid = if class == Class::TreeSet {
                            map_class == Class::TreeMap
                        } else {
                            matches!(map_class, Class::ArrayMap | Class::HashMap)
                        };
                        if !valid {
                            return Err(wasmtime::Error::msg(if class == Class::TreeSet {
                                "Sorted set needs canonical backing tree map"
                            } else {
                                "Hash set needs canonical backing map"
                            }));
                        }
                        let Observation::Map(pairs) = self.map(store, map_class, &map_data, depth)? else {
                            unreachable!()
                        };
                        return Ok(Observation::Set(pairs.into_iter().map(|(key, _)| key).collect()));
                    }
'''+s[b:]
s=s.replace('                    | Class::Subvec => 5,','                    | Class::Subvec\n                    | Class::TreeMap => 5,',1)
s=s.replace('                    | Class::HashSet\n','                    | Class::HashSet\n                    | Class::TreeSet\n',1)
needle='''        let declared = count_field(store, &data[1])?;
        if declared > self.remaining / 2 {'''
assert s.count(needle)==1;s=s.replace(needle,'''        if class == Class::TreeMap {
            return self.tree_map(store, data, depth);
        }
'''+needle)
needle='    fn hash_node(\n';assert s.count(needle)==1
method='''    /// Read the actual canonical node graph in its stored order. No guest
    /// comparator, sequence, printer, equality or collection count is invoked.
    fn tree_map(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        data: &[Val],
        depth: usize,
    ) -> wasmtime::Result<Observation> {
        let declared = count_field(store, &data[2])?;
        if declared > self.remaining / 2 {
            return Err(wasmtime::Error::msg("ABI2 decoder traversal limit"));
        }
        let mut pairs = Vec::with_capacity(declared);
        let mut stack = Vec::new();
        let mut current = data[1].clone();
        loop {
            while sentinel(store, &current)? != Some(0) {
                self.spend(1)?;
                if stack.len() >= 128 {
                    return Err(wasmtime::Error::msg("ABI2 decoder tree path limit"));
                }
                let (class, fields) = self.object(store, &current)?;
                if !matches!(class, Class::BlackNode | Class::RedNode) {
                    return Err(wasmtime::Error::msg("Sorted map needs canonical tree node"));
                }
                current = fields[2].clone();
                stack.push(fields);
            }
            let Some(fields) = stack.pop() else { break };
            if pairs.len() >= declared {
                return Err(wasmtime::Error::msg("Tree map count disagrees with storage"));
            }
            pairs.push((
                self.value(store, &fields[0], depth + 1)?,
                self.value(store, &fields[1], depth + 1)?,
            ));
            current = fields[3].clone();
        }
        if pairs.len() != declared {
            return Err(wasmtime::Error::msg("Tree map count disagrees with storage"));
        }
        Ok(Observation::Map(pairs))
    }

'''
s=s.replace(needle,method+needle);f.write_text(s)
