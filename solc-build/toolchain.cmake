# CMake toolchain: cross-build for SP1's riscv64im-succinct-zkvm-elf with Succinct's GCC (newlib).
set(CMAKE_SYSTEM_NAME Generic)
set(CMAKE_SYSTEM_PROCESSOR riscv64)

if(NOT SOLC_ZK_RISCV OR NOT SOLC_ZK_ROOT)
  message(FATAL_ERROR "set SOLC_ZK_RISCV (GCC toolchain folder) and SOLC_ZK_ROOT (repository folder)")
endif()
set(CMAKE_C_COMPILER "${SOLC_ZK_RISCV}/bin/riscv64-unknown-elf-gcc")
set(CMAKE_CXX_COMPILER "${SOLC_ZK_RISCV}/bin/riscv64-unknown-elf-g++")
set(CMAKE_AR "${SOLC_ZK_RISCV}/bin/riscv64-unknown-elf-ar" CACHE FILEPATH "")
set(CMAKE_RANLIB "${SOLC_ZK_RISCV}/bin/riscv64-unknown-elf-ranlib" CACHE FILEPATH "")
# Nothing can run or link without the zkVM runtime, so CMake checks only compile.
set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)
list(APPEND CMAKE_TRY_COMPILE_PLATFORM_VARIABLES SOLC_ZK_RISCV SOLC_ZK_ROOT)

# Same ISA and ABI as the Rust target. Separate sections, so the final link drops unused code.
# The prefix map removes the local folder from __FILE__ strings, so the program key does not
# depend on where the repository is.
set(ZKVM_FLAGS "-march=rv64im -mabi=lp64 -mcmodel=medany -ffunction-sections -fdata-sections -ffile-prefix-map=${SOLC_ZK_ROOT}=.")
set(CMAKE_C_FLAGS_INIT "${ZKVM_FLAGS}")
# Boost 1.83 uses SIZE_MAX without <cstdint>. With glibc another header includes it, with newlib not.
set(CMAKE_CXX_FLAGS_INIT "${ZKVM_FLAGS} -include cstdint")

set(CMAKE_FIND_ROOT_PATH "${SOLC_ZK_RISCV}/riscv64-unknown-elf")
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE BOTH)
