//! Execute the pinned no_std formatter and the actual immutable GC Decimal.
mod support;
#[path = "support/portable_decode.rs"]
mod portable_decode;
use wasmtime::{Instance, Linker, Module, Store, Val};

#[test]
fn decimal_helper_preserves_primary_scales_and_scratch_bounds() {
    let engine = support::engine();
    let module = Module::new(&engine, include_bytes!("../../../runtime/numeric/artifact/numeric.wasm")).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let memory = instance.get_memory(&mut store, "memory").unwrap();
    let base = memory.data_size(&store);
    memory.grow(&mut store, 1).unwrap();
    let format = instance.get_typed_func::<(i32,i32,i32,i32),i32>(&mut store, "suss_format_decimal").unwrap();
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../../results/m12-decimal-values/format-cases.json")).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 159);
    for case in cases.as_array().unwrap() {
        let digits=case["unscaled"].as_str().unwrap().as_bytes();
        let scale=i32::try_from(case["scale"].as_i64().unwrap()).unwrap();
        let expected=case["expected"].as_str().unwrap().as_bytes();
        let output=base+digits.len()+8;
        let limit=output+digits.len()+16;
        memory.data_mut(&mut store)[base..limit+8].fill(0xa5);
        memory.data_mut(&mut store)[base..base+digits.len()].copy_from_slice(digits);
        let measured=format.call(&mut store,(base as i32,digits.len() as i32,scale,0)).unwrap();
        assert_eq!(measured as usize,expected.len(),"{case}");
        assert!(memory.data(&store)[base+digits.len()..limit+8].iter().all(|&b|b==0xa5));
        let written=format.call(&mut store,(base as i32,digits.len() as i32,scale,output as i32)).unwrap();
        assert_eq!(written,measured,"{case}");
        let bytes=memory.data(&store);
        assert_eq!(&bytes[output..output+written as usize],expected,"{case}");
        assert_eq!(&bytes[base..base+digits.len()],digits,"input overwritten: {case}");
        assert!(bytes[base+digits.len()..output].iter().all(|&b|b==0xa5));
        assert!(bytes[output+written as usize..limit+8].iter().all(|&b|b==0xa5));
    }
}

#[test]
fn compiled_decimal_values_keep_unscaled_fields_scale_text_and_double_bits_after_gc() {
    use suss_compile::{portable, runtime_abi};
    let engine=support::engine();
    let mut store=Store::new(&engine,());
    let runtime=Instance::new(&mut store,&Module::new(&engine,runtime_abi::module()).unwrap(),&[]).unwrap();
    let mut linker=Linker::new(&engine);
    linker.instance(&mut store,"suss.runtime",runtime).unwrap();
    let cases: serde_json::Value=serde_json::from_str(include_str!(
        "../../../results/m12-decimal-values/format-cases.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let unscaled=case["unscaled"].as_str().unwrap();
        let scale=case["scale"].as_i64().unwrap();
        let source=format!("{unscaled}E{}M",-scale);
        let bytes=portable::compile(&source).unwrap();
        runtime_abi::verify_artifact(&bytes,&runtime_abi::Manifest::default()).unwrap();
        let fragment=linker.instantiate(&mut store,&Module::new(&engine,bytes).unwrap()).unwrap();
        let mut output=[Val::null_any_ref()];
        fragment.get_func(&mut store,"eval").unwrap().call(&mut store,&[],&mut output).unwrap();
        let value=output[0].clone();
        store.gc(None).unwrap();
        let decimal=value.unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap();
        let fields=decimal.fields(&mut store).unwrap().collect::<Vec<_>>();
        assert_eq!(fields.len(),2);
        assert_eq!(i64::from(fields[1].unwrap_i32()),scale,"{case}");
        let integer=fields[0].unwrap_anyref().unwrap().as_struct(&store).unwrap().unwrap();
        let fields=integer.fields(&mut store).unwrap().collect::<Vec<_>>();
        let sign=fields[0].unwrap_i32();
        let limbs=fields[1].unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
        let used=fields[2].unwrap_i32() as u32;
        assert!(used<=limbs.len(&store).unwrap());
        let words=(0..used).map(|i|limbs.get(&mut store,i).unwrap().unwrap_i32() as u32).collect::<Vec<_>>();
        assert_eq!(sign==0,words.is_empty());
        assert_ne!(words.last(),Some(&0));
        assert!((-1..=1).contains(&sign));
        assert_eq!(portable_decode::integer_decimal(sign,&words),unscaled,"unscaled field: {case}");
        runtime.get_func(&mut store,"decimal-string").unwrap().call(&mut store,&[value.clone()],&mut output).unwrap();
        let text=output[0].unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();
        let units=text.elems(&mut store).unwrap().map(|v|v.unwrap_i32() as u16).collect::<Vec<_>>();
        assert_eq!(String::from_utf16(&units).unwrap(),case["expected"].as_str().unwrap(),"{case}");
        let mut bits=[Val::F64(0)];
        runtime.get_func(&mut store,"number-unbox").unwrap().call(&mut store,&[value],&mut bits).unwrap();
        assert_eq!(format!("{:016x}",bits[0].unwrap_f64().to_bits()),case["double_bits"].as_str().unwrap(),"{case}");
    }
}

#[test]
fn canonical_binary32_parser_executes_primary_bits_with_bounded_scratch() {
    let engine = support::engine();
    let module = Module::new(&engine,
        include_bytes!("../../../runtime/numeric/artifact/numeric.wasm")).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let memory = instance.get_memory(&mut store, "memory").unwrap();
    let base = memory.data_size(&store) + 16;
    memory.grow(&mut store, 1).unwrap();
    let parse = instance.get_typed_func::<(i32, i32), f64>(&mut store, "suss_parse_decimal_f32").unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-decimal-conversions/float-fields-cases.json")).unwrap();
    for row in cases {
        let units = row["text"].as_str().unwrap().encode_utf16().collect::<Vec<_>>();
        let end = base + units.len() * 2;
        memory.data_mut(&mut store)[base-16..end+16].fill(0xa5);
        for (i, unit) in units.iter().enumerate() {
            memory.data_mut(&mut store)[base+i*2..base+i*2+2].copy_from_slice(&unit.to_le_bytes());
        }
        let actual = parse.call(&mut store, (base as i32, units.len() as i32)).unwrap();
        assert_eq!(format!("{:016x}", actual.to_bits()), row["f64_bits"].as_str().unwrap(), "{row}");
        assert_eq!(format!("{:08x}", (actual as f32).to_bits()), row["f32_bits"].as_str().unwrap(), "{row}");
        let bytes = memory.data(&store);
        assert!(bytes[base-16..base].iter().chain(&bytes[end..end+16]).all(|x| *x==0xa5));
    }
    for text in ["", "1e+", "--1", "1.2.3", "NaN", "Infinity", "0x10", "1M", "\u{d7ff}"] {
        let units = text.encode_utf16().collect::<Vec<_>>();
        for (i, unit) in units.iter().enumerate() {
            memory.data_mut(&mut store)[base+i*2..base+i*2+2].copy_from_slice(&unit.to_le_bytes());
        }
        assert!(parse.call(&mut store, (base as i32, units.len() as i32)).unwrap().is_nan(), "{text}");
    }
}
