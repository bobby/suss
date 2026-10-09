// Original Suss spike code, licensed under the repository MIT/Apache-2.0 terms.
#include "Suss.h"
#include "mlir/IR/DialectImplementation.h"
#include "llvm/ADT/TypeSwitch.h"
#include "Analysis.h"
#include "SussDialect.cpp.inc"
// Explicit list parsing supports zero captures as well as nonempty lists.
static mlir::ParseResult parseCaptureTypes(mlir::AsmParser &parser,
                                           llvm::SmallVector<mlir::Type> &types) {
  return parser.parseCommaSeparatedList(mlir::AsmParser::Delimiter::Square, [&] {
    mlir::Type type;
    if (parser.parseType(type)) return mlir::failure();
    types.push_back(type);
    return mlir::success();
  });
}
static void printCaptureTypes(mlir::AsmPrinter &printer,
                              llvm::ArrayRef<mlir::Type> types) {
  printer << "[";
  llvm::interleaveComma(types, printer, [&](mlir::Type type) { printer << type; });
  printer << "]";
}
#define GET_TYPEDEF_CLASSES
#include "SussTypes.cpp.inc"
#define GET_OP_CLASSES
#include "SussOps.cpp.inc"

void suss::SussDialect::initialize() {
  addTypes<ClosureType, ValueType, EffectClosureType>();
  addOperations<ConstOp, AddOp, ClosureOp, CallOp, ReturnOp, BoolOp, EffectClosureOp, EffectCallOp, ReadOp, WriteOp, DynamicArithmeticOp, EqualOp, IfOp, TryOp, EffectReturnOp, YieldOp, ThrowOp>();
}
mlir::LogicalResult suss::SussDialect::verifyOperationAttribute(
    mlir::Operation *op, mlir::NamedAttribute attr) {
  return suss::analysis::validate(op, attr);
}

// This is a bounded typed spike, not the portable Value/closure ABI. Only
// binary64 and recursively well-formed closure types are admitted here.
static mlir::LogicalResult verifyClosureType(
    llvm::function_ref<mlir::InFlightDiagnostic()> emitError,
    mlir::FunctionType signature, llvm::ArrayRef<mlir::Type> captures);

static mlir::LogicalResult verifyClosureElement(
    llvm::function_ref<mlir::InFlightDiagnostic()> emitError, mlir::Type type) {
  if (mlir::isa<mlir::Float64Type>(type)) return mlir::success();
  if (auto closure = mlir::dyn_cast<suss::ClosureType>(type))
    return verifyClosureType(emitError, closure.getSignature(), closure.getCaptures());
  return emitError() << "closure elements must be f64 or a suss closure type, got " << type;
}

static mlir::LogicalResult verifyClosureType(
    llvm::function_ref<mlir::InFlightDiagnostic()> emitError,
    mlir::FunctionType signature, llvm::ArrayRef<mlir::Type> captures) {
  if (!signature || signature.getNumResults() != 1)
    return emitError() << "closure signature requires exactly one result";
  for (auto type : signature.getInputs())
    if (mlir::failed(verifyClosureElement(emitError, type))) return mlir::failure();
  for (auto type : signature.getResults())
    if (mlir::failed(verifyClosureElement(emitError, type))) return mlir::failure();
  for (auto type : captures)
    if (mlir::failed(verifyClosureElement(emitError, type))) return mlir::failure();
  return mlir::success();
}

mlir::LogicalResult suss::ClosureType::verify(
    llvm::function_ref<mlir::InFlightDiagnostic()> emitError,
    mlir::FunctionType signature, llvm::ArrayRef<mlir::Type> captures) {
  return verifyClosureType(emitError, signature, captures);
}

mlir::LogicalResult suss::ClosureOp::verify() {
  auto type = mlir::dyn_cast<ClosureType>(getResult().getType());
  if (!type) return emitOpError("requires a suss closure result type");
  auto signature = type.getSignature();
  if (signature.getNumResults() != 1)
    return emitOpError("signature requires exactly one result");
  if (getCaptures().getTypes() != type.getCaptures())
    return emitOpError("capture count/type mismatch");
  if (!llvm::hasSingleElement(getBody()))
    return emitOpError("body must contain exactly one block");
  auto &block = getBody().front();
  llvm::SmallVector<mlir::Type> expected(type.getCaptures());
  llvm::append_range(expected, signature.getInputs());
  if (block.getArgumentTypes() != llvm::ArrayRef<mlir::Type>(expected))
    return emitOpError("entry parameters must be captures followed by signature arguments");
  if (block.empty() || !mlir::isa<ReturnOp>(block.back()))
    return emitOpError("body must end in suss.return");
  // The spike requires explicit captures, never implicit SSA closure capture.
  auto isolated = getBody().walk([&](mlir::Operation *op) {
    for (auto operand : op->getOperands())
      if (!getBody().isAncestor(operand.getParentRegion())) {
        emitOpError("body uses an external value; pass it as an explicit capture");
        return mlir::WalkResult::interrupt();
      }
    return mlir::WalkResult::advance();
  });
  if (isolated.wasInterrupted()) return mlir::failure();
  return mlir::success();
}
mlir::LogicalResult suss::CallOp::verify() {
  auto type = mlir::dyn_cast<ClosureType>(getCallee().getType());
  if (!type) return emitOpError("callee must have a suss closure type");
  auto signature = type.getSignature();
  if (getArguments().size() != signature.getNumInputs())
    return emitOpError("call arity mismatch");
  if (getArguments().getTypes() != signature.getInputs())
    return emitOpError("call argument type mismatch");
  if (signature.getNumResults() != 1 || getResult().getType() != signature.getResult(0))
    return emitOpError("call result type mismatch");
  return mlir::success();
}
mlir::LogicalResult suss::ReturnOp::verify() {
  auto parent = mlir::dyn_cast<ClosureOp>((*this)->getParentOp());
  if (!parent) return emitOpError("must terminate a suss.closure body");
  auto type = mlir::dyn_cast<ClosureType>(parent.getResult().getType());
  if (!type || type.getSignature().getNumResults() != 1 ||
      getValue().getType() != type.getSignature().getResult(0))
    return emitOpError("return type mismatch");
  return mlir::success();
}

static bool scalar(mlir::Type t) { return mlir::isa<suss::ValueType, mlir::Float64Type>(t) || t.isInteger(1); }
static bool dynamic(mlir::Type t) { return mlir::isa<suss::ValueType>(t); }
mlir::LogicalResult suss::EffectClosureType::verify(llvm::function_ref<mlir::InFlightDiagnostic()> error, mlir::FunctionType signature, llvm::ArrayRef<mlir::Type> captures) {
 if (!signature || signature.getNumResults()!=1 || !dynamic(signature.getResult(0))) return error()<<"effect closure requires exactly one Value result";
 for(auto t:signature.getInputs()) if(!dynamic(t)) return error()<<"effect closure arguments require Value";
 for(auto t:captures) if(!scalar(t)) return error()<<"effect capture requires f64, i1 or Value";
 if(captures.size()>32 || signature.getNumInputs()>32) return error()<<"effect closure entry bound exceeded";
 return mlir::success();
}
mlir::LogicalResult suss::EffectClosureOp::verify() {
 auto t=mlir::dyn_cast<EffectClosureType>(getResult().getType());
 if(!t || getCaptures().getTypes()!=t.getCaptures()) return emitOpError("effect capture/result mismatch");
 if(!llvm::hasSingleElement(getBody())) return emitOpError("requires exactly one body block");
 llvm::SmallVector<mlir::Type> expected(t.getCaptures()); llvm::append_range(expected,t.getSignature().getInputs());
 auto &b=getBody().front();
 if(b.getArgumentTypes()!=llvm::ArrayRef<mlir::Type>(expected)) return emitOpError("entry type mismatch");
 if(b.empty() || !mlir::isa<EffectReturnOp,ThrowOp>(b.back())) return emitOpError("requires effect_return or throw");
 auto walk=getBody().walk([&](mlir::Operation *op){for(auto v:op->getOperands()) if(!getBody().isAncestor(v.getParentRegion())) return mlir::WalkResult::interrupt(); return mlir::WalkResult::advance();});
 if(walk.wasInterrupted()) return emitOpError("external value requires explicit capture");
 return mlir::success();
}
mlir::LogicalResult suss::EffectCallOp::verify() {
 auto t=mlir::dyn_cast<EffectClosureType>(getCallee().getType());
 if(!t || !dynamic(getResult().getType()) || getArguments().size()!=t.getSignature().getNumInputs()) return emitOpError("effect call arity/type mismatch");
 for(auto v:getArguments()) if(!scalar(v.getType())) return emitOpError("effect call argument requires scalar");
 return mlir::success();
}
mlir::LogicalResult suss::ReadOp::verify(){if(!dynamic(getResult().getType()) || getNamespaceName().empty() || getName().empty()) return emitOpError("read requires named binding and Value result"); return mlir::success();}
mlir::LogicalResult suss::WriteOp::verify(){if(!dynamic(getResult().getType()) || !scalar(getValue().getType()) || getNamespaceName().empty() || getName().empty()) return emitOpError("write requires named scalar binding and Value result"); return mlir::success();}
mlir::LogicalResult suss::DynamicArithmeticOp::verify(){auto numeric=[](mlir::Type t){return dynamic(t)||mlir::isa<mlir::Float64Type>(t);}; if((getKind()!= "add" && getKind()!= "multiply") || !numeric(getLhs().getType()) || !numeric(getRhs().getType()) || !dynamic(getResult().getType()) || (!dynamic(getLhs().getType())&&!dynamic(getRhs().getType()))) return emitOpError("dynamic arithmetic requires add/multiply, a Value operand and numeric/Value operands"); return mlir::success();}
mlir::LogicalResult suss::EqualOp::verify(){if(!scalar(getLhs().getType())||!scalar(getRhs().getType())) return emitOpError("equality requires scalar operands"); return mlir::success();}
mlir::LogicalResult suss::IfOp::verify(){if(!dynamic(getResult().getType())) return emitOpError("if result requires Value"); for(auto *r:{&getConsequent(),&getAlternative()}) if(!llvm::hasSingleElement(*r)||r->front().getNumArguments()!=0||r->front().empty()||!mlir::isa<YieldOp>(r->front().back())) return emitOpError("if arms require one argument-free block ending in yield"); return mlir::success();}
mlir::LogicalResult suss::TryOp::verify(){if(!dynamic(getResult().getType())) return emitOpError("try result requires Value"); unsigned i=0; for(auto v:{getBody(),getHandler(),getCleanup()}){auto t=mlir::dyn_cast<EffectClosureType>(v.getType()); if(!t||t.getSignature().getNumInputs()!=(i==1?1:0)) return emitOpError("try requires effect closures with arities 0/1/0"); ++i;} return mlir::success();}
mlir::LogicalResult suss::EffectReturnOp::verify(){if(!mlir::isa<EffectClosureOp>((*this)->getParentOp())||!scalar(getValue().getType())) return emitOpError("effect return requires closure parent and scalar"); return mlir::success();}
mlir::LogicalResult suss::YieldOp::verify(){if(!mlir::isa<IfOp>((*this)->getParentOp())||!scalar(getValue().getType())) return emitOpError("yield requires if parent and scalar"); return mlir::success();}
mlir::LogicalResult suss::ThrowOp::verify(){if(!mlir::isa<EffectClosureOp>((*this)->getParentOp())) return emitOpError("throw requires effect closure parent"); return mlir::success();}
