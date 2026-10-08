cmake_minimum_required(VERSION 3.20)
project(SussMLIRSpike LANGUAGES C CXX)
find_package(MLIR REQUIRED CONFIG)
list(APPEND CMAKE_MODULE_PATH "${MLIR_CMAKE_DIR}" "${LLVM_CMAKE_DIR}")
include(TableGen)
include(AddLLVM)
include(AddMLIR)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
include_directories(SYSTEM ${LLVM_INCLUDE_DIRS} ${MLIR_INCLUDE_DIRS})
include_directories(${CMAKE_CURRENT_SOURCE_DIR}/src ${CMAKE_CURRENT_BINARY_DIR})
add_definitions(${LLVM_DEFINITIONS})
set(LLVM_TARGET_DEFINITIONS src/Suss.td)
mlir_tablegen(SussDialect.h.inc -gen-dialect-decls -dialect=suss)
mlir_tablegen(SussDialect.cpp.inc -gen-dialect-defs -dialect=suss)
mlir_tablegen(SussTypes.h.inc -gen-typedef-decls -typedefs-dialect=suss)
mlir_tablegen(SussTypes.cpp.inc -gen-typedef-defs -typedefs-dialect=suss)
mlir_tablegen(SussOps.h.inc -gen-op-decls)
mlir_tablegen(SussOps.cpp.inc -gen-op-defs)
add_public_tablegen_target(SussGenerated)
add_executable(suss-mlir-opt src/Suss.cpp src/main.cpp)
add_dependencies(suss-mlir-opt SussGenerated)
llvm_update_compile_flags(suss-mlir-opt)
target_link_libraries(suss-mlir-opt PRIVATE MLIRIR MLIRSupport MLIROptLib)

find_package(Python3 REQUIRED COMPONENTS Interpreter)
enable_testing()
add_test(NAME suss-dialect-fixtures COMMAND ${Python3_EXECUTABLE}
  ${CMAKE_CURRENT_SOURCE_DIR}/fixtures/check.py $<TARGET_FILE:suss-mlir-opt>)

# Separate verified-graph transport tool; opt and dialect sources remain unchanged.
add_executable(suss-mlir-export src/Suss.cpp src/Export.cpp src/export-main.cpp)
add_dependencies(suss-mlir-export SussGenerated)
llvm_update_compile_flags(suss-mlir-export)
target_compile_options(suss-mlir-export PRIVATE -fexceptions)
target_link_libraries(suss-mlir-export PRIVATE MLIRIR MLIRSupport MLIRParser)
add_test(NAME suss-verified-graph-export COMMAND ${Python3_EXECUTABLE}
  ${CMAKE_CURRENT_SOURCE_DIR}/bridge/export-fixtures/check-export.py
  $<TARGET_FILE:suss-mlir-export>)

add_test(NAME suss-effects-export COMMAND ${Python3_EXECUTABLE}
  ${CMAKE_CURRENT_SOURCE_DIR}/bridge/effects-fixtures/check-effects.py
  $<TARGET_FILE:suss-mlir-export>)
