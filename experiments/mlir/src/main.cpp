// Original Suss spike code, licensed under the repository MIT/Apache-2.0 terms.
#include "Suss.h"
#include "mlir/Tools/mlir-opt/MlirOptMain.h"
int main(int argc, char **argv) {
  mlir::DialectRegistry registry;
  registry.insert<suss::SussDialect>();
  return mlir::asMainReturnCode(mlir::MlirOptMain(argc, argv, "Suss bounded dialect verifier\n", registry));
}
