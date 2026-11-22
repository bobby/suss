from pathlib import Path
root=Path(__file__).parent
parts=[]
for name in ['add','mul']:
    cap='(i32.add (local.get $la) (local.get $lb))' if name=='mul' else '(i32.add (select (local.get $la) (local.get $lb) (i32.gt_u (local.get $la) (local.get $lb))) (i32.const 1))'
    parts.append(f'''(func $bridge_{name} (param $a (ref $limbs)) (param $b (ref $limbs)) (result (ref $limbs) i32)
      (local $la i32) (local $lb i32) (local $used i32) (local $bp i32) (local $out i32) (local $r (ref null $limbs))
      (local.set $la (array.len (local.get $a))) (local.set $lb (array.len (local.get $b)))
      (local.set $bp (i32.add (i32.const 2097152) (i32.shl (local.get $la) (i32.const 2))))
      (local.set $out (i32.add (local.get $bp) (i32.shl (local.get $lb) (i32.const 2))))
      (call $ensure (i32.add (local.get $out) (i32.shl {cap} (i32.const 2))))
      (call $to_memory (local.get $a) (i32.const 2097152))
      (call $to_memory (local.get $b) (local.get $bp))
      (global.set $stack (i32.const STACK_TOP))
      (local.set $used (call ${name} (i32.const 2097152) (local.get $la) (local.get $bp) (local.get $lb) (local.get $out)))
      (call $from_memory (local.get $out) {cap}) (local.get $used))''')
for name in ['control','gc_add','gc_mul','bridge_add','bridge_mul','linear_add','linear_mul','linear_control','allocate']:
    if name == 'linear_control':
        call='(local.set $used (array.len (local.get $a)))'
        low='(i32.load (i32.const 2097152))'
    elif name.startswith('linear_'):
        operation=name.removeprefix('linear_')
        call=f'(global.set $stack (i32.const STACK_TOP)) (local.set $used (call ${operation} (i32.const 2097152) (array.len (local.get $a)) (i32.const 2162688) (array.len (local.get $b)) (i32.const 2228224)))'
        low='(i32.load (i32.const 2228224))'
    else:
        call='(local.get $a) (array.len (local.get $a))' if name=='control' else (
            '(array.new $limbs (i32.const 1) (array.len (local.get $a))) (array.len (local.get $a))' if name=='allocate' else f'(call ${name} (ref.as_non_null (local.get $a)) (ref.as_non_null (local.get $b)))')
        call+=' (local.set $used) (local.set $r) (global.set $sink (local.get $r))'
        low='(array.get $limbs (local.get $r) (i32.const 0))'
    parts.append(f'''(func (export "{name}") (param $n i32) (result i64)
      (local $a (ref null $limbs)) (local $b (ref null $limbs)) (local $r (ref null $limbs))
      (local $i i32) (local $used i32) (local $sum i64)
      (local.set $a (global.get $a)) (local.set $b (global.get $b))
      (block $done (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        {call}
        (local.set $sum (i64.add (local.get $sum) (i64.add (i64.extend_i32_u (local.get $used))
          (if (result i64) (local.get $used) (then (i64.extend_i32_u {low})) (else (i64.const 0))))))
        (local.set $i (i32.add (local.get $i) (i32.const 1))) (br $next)))
      (global.set $used (local.get $used)) (local.get $sum))''')
import sys
text=(root/'bigint.wat').read_text().replace('  ;; GENERATED','\n'.join(parts)).replace('STACK_TOP', sys.argv[1])
(root/'bigint-generated.wat').write_text(text)
