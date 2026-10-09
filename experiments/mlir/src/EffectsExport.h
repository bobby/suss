// Original isolated evaluation code; repository MIT/Apache-2.0 terms.
#pragma once
#include <set>
namespace {
llvm::json::Object effectType(mlir::Type t) {
 if(t.isF64()) return llvm::json::Object{{"kind","f64"}};
 if(t.isInteger(1)) return llvm::json::Object{{"kind","bool"}};
 if(mlir::isa<suss::ValueType>(t)) return llvm::json::Object{{"kind","value"}};
 auto c=mlir::dyn_cast<suss::EffectClosureType>(t); need(bool(c),"unsupported effects type");
 llvm::json::Array captures; for(auto ct:c.getCaptures()) captures.push_back(effectType(ct));
 return llvm::json::Object{{"kind","effect_closure"},{"arity",int64_t(c.getSignature().getNumInputs())},{"captures",std::move(captures)}};
}
void effectAttrs(mlir::Operation *op, llvm::ArrayRef<llvm::StringRef> names={}) {
 for(auto a:op->getAttrs()) need(llvm::is_contained(names,a.getName().strref()),"unsupported effects attribute");
}
struct EffectsExporter {
 size_t remaining=8192; int64_t next=0;
 llvm::json::Object region(mlir::Region &r, llvm::DenseMap<mlir::Value,int64_t> ids, unsigned depth, bool arm=false) {
  need(depth<=16 && llvm::hasSingleElement(r),"effects region/depth bound");
  auto &b=r.front(); need(!b.empty() && b.getNumArguments()<=64 && b.getOperations().size()<=4097,"effects block bound");
  llvm::json::Array parameters,operations;
  for(auto arg:b.getArguments()){need(remaining>0,"effects value bound");--remaining;auto id=next++;ids[arg]=id;parameters.push_back(llvm::json::Object{{"id",id},{"type",effectType(arg.getType())}});}
  auto ref=[&](mlir::Value v){auto it=ids.find(v);need(it!=ids.end(),"effects operand must dominate use");return it->second;};
  for(auto &op:b){
   if(mlir::isa<suss::EffectReturnOp,suss::ThrowOp,suss::YieldOp>(op)) {
    effectAttrs(&op);need(&op==&b.back(),"early effects terminator");
    need(arm==mlir::isa<suss::YieldOp>(op),"effects terminator context");
    auto kind=mlir::isa<suss::ThrowOp>(op)?"throw":arm?"yield":"return";
    return llvm::json::Object{{"parameters",std::move(parameters)},{"operations",std::move(operations)},{"terminator",llvm::json::Object{{"op",kind},{"value",ref(op.getOperand(0))}}}};
   }
   need(remaining>0 && op.getNumResults()==1,"effects operation/result bound");--remaining;
   auto id=next++;llvm::json::Object item{{"id",id},{"result_type",effectType(op.getResult(0).getType())}};
   if(auto c=mlir::dyn_cast<suss::ConstOp>(op)){effectAttrs(&op,{"value"}); item["op"]="const"; auto bits=c.getValue().bitcastToAPInt().getZExtValue();std::string text;llvm::raw_string_ostream stream(text);stream<<llvm::format_hex_no_prefix(bits,16,false);item["bits"]=stream.str();}
   else if(auto c=mlir::dyn_cast<suss::BoolOp>(op)){effectAttrs(&op,{"value"});item["op"]="bool";item["value"]=c.getValue();}
   else if(mlir::isa<suss::AddOp,suss::DynamicArithmeticOp,suss::EqualOp>(op)){effectAttrs(&op,mlir::isa<suss::DynamicArithmeticOp>(op)?llvm::ArrayRef<llvm::StringRef>{"kind"}:llvm::ArrayRef<llvm::StringRef>{});item["op"]=mlir::isa<suss::EqualOp>(op)?"equal":mlir::isa<suss::AddOp>(op)?"add":"dynamic_arithmetic";if(auto a=mlir::dyn_cast<suss::DynamicArithmeticOp>(op))item["kind"]=a.getKind();item["lhs"]=ref(op.getOperand(0));item["rhs"]=ref(op.getOperand(1));}
   else if(mlir::isa<suss::ReadOp,suss::WriteOp>(op)){effectAttrs(&op,{"namespace_name","name"});item["op"]=mlir::isa<suss::ReadOp>(op)?"read":"write";item["namespace"]=op.getAttrOfType<mlir::StringAttr>("namespace_name").getValue();item["name"]=op.getAttrOfType<mlir::StringAttr>("name").getValue();if(mlir::isa<suss::WriteOp>(op))item["value"]=ref(op.getOperand(0));}
   else if(auto c=mlir::dyn_cast<suss::EffectClosureOp>(op)){effectAttrs(&op);item["op"]="closure";llvm::json::Array captures;for(auto v:c.getCaptures())captures.push_back(ref(v));item["captures"]=std::move(captures);auto saved=next;next=0;item["function"]=region(c.getBody(),llvm::DenseMap<mlir::Value,int64_t>(),depth+1);next=saved;}
   else if(auto c=mlir::dyn_cast<suss::EffectCallOp>(op)){effectAttrs(&op);item["op"]="call";item["callee"]=ref(c.getCallee());llvm::json::Array args;for(auto v:c.getArguments())args.push_back(ref(v));item["arguments"]=std::move(args);}
   else if(auto c=mlir::dyn_cast<suss::IfOp>(op)){effectAttrs(&op);item["op"]="if";item["condition"]=ref(c.getCondition());item["consequent"]=region(c.getConsequent(),ids,depth+1,true);item["alternative"]=region(c.getAlternative(),ids,depth+1,true);}
   else if(auto c=mlir::dyn_cast<suss::TryOp>(op)){effectAttrs(&op);item["op"]="try";item["body"]=ref(c.getBody());item["handler"]=ref(c.getHandler());item["cleanup"]=ref(c.getCleanup());}
   else need(false,"unsupported effects operation");
   ids[op.getResult(0)]=id;operations.push_back(std::move(item));
  }
  throw std::runtime_error("effects region missing terminator");
 }
};
}
llvm::json::Object suss::exportVerifiedEffects(mlir::ModuleOp module) {
 need(mlir::succeeded(mlir::verify(module)),"effects module verification failed");effectAttrs(module,{"suss.cells"});
 llvm::json::Array cells; std::set<std::pair<std::string,std::string>> declared;
 auto declarations=module->getAttrOfType<mlir::ArrayAttr>("suss.cells"); need(bool(declarations) && declarations.size()<=32,"effects requires bounded cell declarations");
 for(auto a:declarations){auto d=mlir::dyn_cast<mlir::DictionaryAttr>(a);need(bool(d)&&d.size()==3,"invalid cell declaration");auto ns=mlir::dyn_cast_or_null<mlir::StringAttr>(d.get("namespace"));auto name=mlir::dyn_cast_or_null<mlir::StringAttr>(d.get("name"));auto bits=mlir::dyn_cast_or_null<mlir::StringAttr>(d.get("initial_bits"));need(ns&&name&&bits&&!ns.getValue().empty()&&!name.getValue().empty()&&bits.getValue().size()==16,"invalid cell fields");for(char ch:bits.getValue())need((ch>='0'&&ch<='9')||(ch>='a'&&ch<='f'),"invalid cell bits");need(declared.emplace(ns.getValue().str(),name.getValue().str()).second,"duplicate cell declaration");cells.push_back(llvm::json::Object{{"namespace",ns.getValue()},{"name",name.getValue()},{"initial_bits",bits.getValue()}});}
 module.walk([&](mlir::Operation *op){if(mlir::isa<suss::ReadOp,suss::WriteOp>(op))need(declared.count({op->getAttrOfType<mlir::StringAttr>("namespace_name").getValue().str(),op->getAttrOfType<mlir::StringAttr>("name").getValue().str()}),"undeclared effects binding");});
 need(llvm::hasSingleElement(module.getBody()->getOperations()),"effects requires exactly one root closure");
 auto root=mlir::dyn_cast<suss::EffectClosureOp>(module.getBody()->front());need(bool(root),"effects root requires effect closure");effectAttrs(root);
 auto type=mlir::cast<suss::EffectClosureType>(root.getResult().getType());need(root.getCaptures().empty() && type.getSignature().getNumInputs()==0 && root.getResult().use_empty(),"effects entry must be closed arity zero");
 EffectsExporter exporter;
 return llvm::json::Object{{"schema","suss.mlir.effects.v1"},{"cells",std::move(cells)},{"entry",exporter.region(root.getBody(),llvm::DenseMap<mlir::Value,int64_t>(),0)}};
}
