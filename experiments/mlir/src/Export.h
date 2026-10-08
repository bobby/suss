// Original evaluation code, repository MIT/Apache-2.0 terms.
#pragma once
#include "mlir/IR/BuiltinOps.h"
#include "llvm/Support/JSON.h"
namespace suss {
llvm::json::Object exportVerifiedEffects(mlir::ModuleOp module);
// Throws std::runtime_error on unsupported transport features. Re-verifies module.
llvm::json::Object exportVerifiedGraph(mlir::ModuleOp module,
                                      llvm::StringRef expectedBits,
                                      bool preserveSourceAnalysis = false);
}
