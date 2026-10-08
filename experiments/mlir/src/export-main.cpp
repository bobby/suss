// Original evaluation code, repository MIT/Apache-2.0 terms.
#include "Export.h"
#include "Suss.h"
#include "mlir/Parser/Parser.h"
#include "llvm/Support/CommandLine.h"
#include "llvm/Support/FileSystem.h"
#include "llvm/Support/MemoryBuffer.h"
#include "llvm/Support/SourceMgr.h"
#include <exception>
#include <stdexcept>
#include "llvm/Support/FormatVariadic.h"
static llvm::cl::opt<std::string> input(llvm::cl::Positional, llvm::cl::Required, llvm::cl::desc("verified-graph input MLIR"));
static llvm::cl::opt<std::string> output("export-graph", llvm::cl::desc("JSON output path (default stdout)"), llvm::cl::init("-"));
static llvm::cl::opt<std::string> expected("expected-bits", llvm::cl::desc("independent expected f64 bits, 16 lowercase hex digits"));
static llvm::cl::opt<bool> effects("effects", llvm::cl::desc("export strict effects lane"));
static llvm::cl::opt<bool> preserveSource("preserve-source-analysis", llvm::cl::desc("export verified original source facts in the numeric v2 lane"));
int main(int argc, char **argv) {
  llvm::cl::ParseCommandLineOptions(argc, argv, "Suss verified graph exporter\n");
  try {
    auto bytes = llvm::MemoryBuffer::getFile(input);
    if (!bytes) throw std::runtime_error("cannot read input");
    if ((*bytes)->getBufferSize() > 1048576) throw std::runtime_error("input exceeds 1 MiB");
    mlir::DialectRegistry registry; registry.insert<suss::SussDialect>();
    mlir::MLIRContext context(registry); context.allowUnregisteredDialects(false);
    llvm::SourceMgr sources; sources.AddNewSourceBuffer(std::move(*bytes), llvm::SMLoc());
    auto module = mlir::parseSourceFile<mlir::ModuleOp>(sources, &context);
    if (!module) throw std::runtime_error("registered MLIR parse failed");
    if (effects && !expected.empty()) throw std::runtime_error("effects oracle is separate; expected-bits unsupported");
    if (effects && preserveSource) throw std::runtime_error("source v2 is a separate numeric lane");
    auto graph = effects ? suss::exportVerifiedEffects(*module) : suss::exportVerifiedGraph(*module, expected, preserveSource);
    // No output file is opened until parsing, verification and export checks succeed.
    std::error_code error;
    llvm::raw_fd_ostream stream(output, error, llvm::sys::fs::OF_Text);
    if (error) throw std::runtime_error(error.message());
    stream << llvm::formatv("{0:2}\n", llvm::json::Value(std::move(graph)));
    stream.flush();
    if (stream.has_error()) throw std::runtime_error("JSON output write failed");
    return 0;
  } catch (const std::exception &error) { llvm::errs() << "export failed: " << error.what() << '\n'; return 1; }
}
