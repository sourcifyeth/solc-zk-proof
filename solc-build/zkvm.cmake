# Runs right after solc's project() (CMAKE_PROJECT_solidity_INCLUDE). It adds only what the
# zkVM does not have. solc's own CMake files stay unchanged.

# newlib has no threads. solc links Threads::Threads, but it never starts a thread.
if(NOT TARGET Threads::Threads)
  add_library(Threads::Threads INTERFACE IMPORTED)
endif()

# Stand-in headers for boost::process and termios (see the comments in each file).
include_directories(BEFORE SYSTEM "${CMAKE_CURRENT_LIST_DIR}/shims/include")
