// Original Suss spike code, licensed under the repository MIT/Apache-2.0 terms.
#pragma once
#include "mlir/IR/Builders.h"
#include "mlir/IR/ImplicitLocOpBuilder.h"
#include "mlir/Bytecode/BytecodeOpInterface.h"
#include "mlir/IR/Dialect.h"
#include "mlir/IR/OpDefinition.h"
#include "mlir/IR/BuiltinTypes.h"
#include "SussDialect.h.inc"
#define GET_TYPEDEF_CLASSES
#include "SussTypes.h.inc"
#define GET_OP_CLASSES
#include "SussOps.h.inc"
