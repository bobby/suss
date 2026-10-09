// Original evaluation code, repository MIT/Apache-2.0 terms.
#include "Export.h"
#include "Suss.h"
#include "mlir/IR/Verifier.h"
#include "llvm/ADT/DenseMap.h"
#include "llvm/Support/Format.h"
#include <stdexcept>
using namespace mlir;
namespace {
void need(bool condition, const char *message) {
  if (!condition) throw std::runtime_error(message);
}
void attrs(Operation *op, llvm::StringRef allowed = {}) {
  for (auto attr : op->getAttrs())
    need(attr.getName().strref() == allowed,
         "unsupported attribute (source-analysis payload transport pending)");
}
suss::ClosureType closure(Type type) {
  auto c = dyn_cast<suss::ClosureType>(type);
  need(bool(c), "expected registered suss closure type");
  auto sig = c.getSignature();
  need(sig.getNumInputs() <= 32 && sig.getNumResults() == 1 &&
       c.getCaptures().size() <= 32, "unsupported closure signature shape");
  return c;
}
llvm::json::Object numericClosureType(Type type) {
  if (type.isF64()) return llvm::json::Object{{"kind", "f64"}};
  auto c = closure(type);
  for (auto t : c.getSignature().getInputs()) need(t.isF64(), "only f64 closure arguments supported");
  need(c.getSignature().getResult(0).isF64(), "only f64 closure results supported");
  llvm::json::Array captures;
  for (auto t : c.getCaptures()) {
    need(t.isF64(), "only f64 captures supported");
    captures.push_back(llvm::json::Object{{"kind", "f64"}});
  }
  return llvm::json::Object{{"kind", "closure"}, {"arity", int64_t(c.getSignature().getNumInputs())}, {"captures", std::move(captures)}};
}
struct Exporter {
  size_t remaining = 8192;
  llvm::json::Object graph(Region &region, unsigned depth) {
    need(depth <= 1 && llvm::hasSingleElement(region), "unsupported region shape/nested closure");
    auto &block = region.front();
    need(block.getNumArguments() <= 32 && !block.empty(), "unsupported entry shape");
    need(block.getOperations().size() <= 4097, "operation limit exceeded");
    llvm::DenseMap<Value, int64_t> ids;
    llvm::json::Array parameters, operations;
    auto define = [&](Value value) {
      need(remaining > 0, "total value limit exceeded"); --remaining;
      int64_t id = ids.size(); need(ids.try_emplace(value, id).second, "duplicate SSA definition");
    };
    auto use = [&](Value value) -> int64_t {
      auto it = ids.find(value);
      need(it != ids.end(), "implicit capture/forward SSA use unsupported");
      return it->second;
    };
    for (auto arg : block.getArguments()) { parameters.push_back(numericClosureType(arg.getType())); define(arg); }
    for (auto &op : block) {
      need(op.getNumSuccessors() == 0, "successors unsupported");
      if (auto ret = dyn_cast<suss::ReturnOp>(&op)) {
        attrs(&op);
        need(&op == &block.back() && op.getNumRegions() == 0 && op.getNumResults() == 0, "return must be final");
        return llvm::json::Object{{"parameters", std::move(parameters)}, {"operations", std::move(operations)}, {"return_value", use(ret.getValue())}};
      }
      need(op.getNumResults() == 1, "operation must have one result");
      llvm::json::Object node;
      if (auto constant = dyn_cast<suss::ConstOp>(&op)) {
        attrs(&op, "value");
        need(op.getNumOperands() == 0 && op.getNumRegions() == 0 && constant.getResult().getType().isF64(), "unsupported constant shape");
        auto value = op.getAttrOfType<FloatAttr>("value");
        need(bool(value) && value.getType().isF64(), "constant must have f64 attribute");
        uint64_t raw = value.getValue().bitcastToAPInt().getZExtValue();
        std::string bits; llvm::raw_string_ostream out(bits);
        out << llvm::format_hex_no_prefix(raw, 16, false); out.flush();
        node = llvm::json::Object{{"op", "suss.const"}, {"bits", bits}};
      } else if (auto add = dyn_cast<suss::AddOp>(&op)) {
        attrs(&op); need(op.getNumRegions() == 0, "add regions unsupported");
        need(add.getLhs().getType().isF64() && add.getRhs().getType().isF64() && add.getResult().getType().isF64(), "add requires f64");
        node = llvm::json::Object{{"op", "suss.add"}, {"lhs", use(add.getLhs())}, {"rhs", use(add.getRhs())}};
      } else if (auto c = dyn_cast<suss::ClosureOp>(&op)) {
        attrs(&op); need(depth == 0 && op.getNumRegions() == 1, "nested closure construction unsupported");
        auto type = closure(c.getResult().getType());
        (void)numericClosureType(type);
        llvm::json::Array captures;
        for (auto value : c.getCaptures()) { need(value.getType().isF64(), "only f64 capture operands supported"); captures.push_back(use(value)); }
        node = llvm::json::Object{{"op", "suss.closure"}, {"captures", std::move(captures)}, {"arity", int64_t(type.getSignature().getNumInputs())}, {"body", graph(c.getBody(), depth + 1)}};
      } else if (auto call = dyn_cast<suss::CallOp>(&op)) {
        attrs(&op); need(op.getNumRegions() == 0, "call regions unsupported");
        (void)numericClosureType(call.getCallee().getType());
        need(call.getResult().getType().isF64(), "call result must be f64");
        llvm::json::Array args;
        for (auto value : call.getArguments()) { need(value.getType().isF64(), "call argument must be f64"); args.push_back(use(value)); }
        node = llvm::json::Object{{"op", "suss.call"}, {"callee", use(call.getCallee())}, {"arguments", std::move(args)}};
      } else { throw std::runtime_error("unsupported operation in exported graph"); }
      operations.push_back(std::move(node)); define(op.getResult(0));
    }
    throw std::runtime_error("missing suss.return");
  }
};
}
llvm::json::Object suss::exportVerifiedGraph(ModuleOp module, llvm::StringRef expectedBits, bool preserveSourceAnalysis) {
  need(succeeded(verify(module)), "MLIR verification failed");
  need(expectedBits.size() == 16 && llvm::all_of(expectedBits, [](char c) { return (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f'); }), "expected bits require 16 lowercase hex digits");
  attrs(module.getOperation(), preserveSourceAnalysis ? "suss.source_analysis" : "");
  auto source = module->getAttrOfType<StringAttr>("suss.source_analysis");
  need(!preserveSourceAnalysis || bool(source), "source-preserving export requires verified source-analysis string");
  auto *block = module.getBody();
  need(block->getNumArguments() == 0 && block->getOperations().size() == 2, "module must contain exactly producer and caller closure wrappers");
  auto producer = dyn_cast<ClosureOp>(&block->front());
  auto caller = dyn_cast<ClosureOp>(&block->back());
  need(bool(producer) && bool(caller), "root wrappers must be registered suss.closure");
  attrs(producer.getOperation()); attrs(caller.getOperation());
  need(producer.getCaptures().empty() && caller.getCaptures().empty() && producer.getResult().use_empty() && caller.getResult().use_empty(), "root wrappers must be closed and unused");
  auto pt = closure(producer.getResult().getType());
  auto ct = closure(caller.getResult().getType());
  need(pt.getCaptures().empty() && ct.getCaptures().empty() && pt.getSignature().getNumInputs() == 0 && ct.getSignature().getNumInputs() == 1 && ct.getSignature().getResult(0).isF64(), "root wrapper signature mismatch");
  need(pt.getSignature().getResult(0) == ct.getSignature().getInput(0), "caller binding type differs from producer result");
  need(isa<ClosureType>(pt.getSignature().getResult(0)), "producer must return closure for harness binding");
  (void)numericClosureType(pt.getSignature().getResult(0));
  Exporter exporter;
  auto p = exporter.graph(producer.getBody(), 0);
  auto c = exporter.graph(caller.getBody(), 0);
  llvm::json::Object result{{"schema", preserveSourceAnalysis ? "suss.mlir.bridge.source.v2" : "suss.mlir.bridge.v1"}, {"verified_mlir", true}, {"producer", std::move(p)}, {"caller", std::move(c)}, {"expected_bits", expectedBits.str()}};
  // Preserve the entire already-verified native facts payload, never infer it
  // from the numeric graph or synthesize missing source children.
  if (preserveSourceAnalysis) result["source_analysis"] = source.getValue().str();
  return result;
}

#include "EffectsExport.h"
