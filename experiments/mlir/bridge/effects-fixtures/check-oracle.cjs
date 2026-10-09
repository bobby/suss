// Original bounded development oracle, repository MIT/Apache-2.0 terms.
// This executes JS and an exported-graph model, not the Wasm emitter/runtime.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
assert.equal(process.version,'v24.5.0','use the pinned Node development oracle');
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
function source(thrown,override,truth){
 let journal=0,count=0;const mark=n=>{journal=journal*10+n;};
 const condition=()=>{mark(1);count++;return truth;};
 const body=()=>{let value;if(condition()===true){mark(2);value=7;}else{mark(7);value=8;}if(thrown)throw 17;return value;};
 const handler=x=>{mark(3);return x;};const cleanup=()=>{mark(4);if(override)throw 99;return 0;};
 const inner=()=>{try{return body();}catch(x){return handler(x);}finally{cleanup();}};
 let value;if(override){try{value=inner();}catch(x){mark(5);value=x;}finally{mark(6);}}else value=inner();
 return {result_bits:bits(value),journal_bits:bits(journal),count_bits:bits(count)};
}
function graph(g){
 const cells=new Map(g.cells.map(c=>[c.namespace+'/'+c.name,Buffer.from(c.initial_bits,'hex').readDoubleBE()]));
 function region(r,args=[],outer=new Map()){
  const values=new Map(outer);r.parameters.forEach((p,i)=>values.set(p.id,args[i]));const v=id=>{assert(values.has(id));return values.get(id);};
  for(const o of r.operations){let x;switch(o.op){
   case 'const':x=Buffer.from(o.bits,'hex').readDoubleBE();break;
   case 'bool':x=o.value;break;
   case 'add':x=v(o.lhs)+v(o.rhs);break;
   case 'dynamic_arithmetic':assert.equal(typeof v(o.lhs),'number');assert.equal(typeof v(o.rhs),'number');x=o.kind==='add'?v(o.lhs)+v(o.rhs):v(o.lhs)*v(o.rhs);break;
   case 'equal':x=v(o.lhs)===v(o.rhs);break;
   case 'read':x=cells.get(o.namespace+'/'+o.name);break;
   case 'write':x=v(o.value);cells.set(o.namespace+'/'+o.name,x);break;
   case 'closure':{const captures=o.captures.map(v);x=(...a)=>region(o.function,[...captures,...a]);break;}
   case 'call':x=v(o.callee)(...o.arguments.map(v));break;
   case 'if':x=region(v(o.condition)?o.consequent:o.alternative,[],values);break;
   case 'try':{const body=v(o.body),handler=v(o.handler),cleanup=v(o.cleanup);try{x=body();}catch(e){x=handler(e);}finally{cleanup();}break;}
   default:throw Error('unsupported model operation '+o.op);
  }values.set(o.id,x);}
  const result=v(r.terminator.value);if(r.terminator.op==='throw')throw result;return result;
 }
 const value=region(g.entry);return {result_bits:bits(value),journal_bits:bits(cells.get('effects/journal')),count_bits:bits(cells.get('effects/count'))};
}
const cases={'success':[false,false,true],'alternative':[false,false,false],'handled-throw':[true,false,true],'cleanup-overrides-success':[false,true,true],'cleanup-overrides-handler':[true,true,true],'captured-f64':[false,false,true],'nested-join':[false,false,true]};
const results={};for(const [name,args] of Object.entries(cases)){const expected=source(...args);assert.deepEqual(graph(JSON.parse(fs.readFileSync(path.join(__dirname,name+'.json'),'utf8'))),expected,name);results[name]=expected;}
assert.deepEqual(results,JSON.parse(fs.readFileSync(path.join(__dirname,'expected.json'),'utf8')),'frozen expected bits');
console.log('PASS: pinned Node v24.5.0 source oracle vs 7 actual exported graph models; order/once/cleanup override; Wasmtime pending');
