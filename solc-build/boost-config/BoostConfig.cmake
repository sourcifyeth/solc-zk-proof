# Boost 1.83 for the zkVM build: headers from the release tarball, Boost.Filesystem built by
# scripts/build.sh. The other compiled components are for solc's CLI and tests only.
if(NOT SOLC_ZK_BOOST)
  message(FATAL_ERROR "set SOLC_ZK_BOOST to the Boost source folder")
endif()
set(Boost_FOUND TRUE)
set(Boost_VERSION "1.83.0")
set(Boost_VERSION_STRING "1.83.0")
set(Boost_INCLUDE_DIRS "${SOLC_ZK_BOOST}")

foreach(name headers boost filesystem unit_test_framework program_options)
  if(NOT TARGET Boost::${name})
    add_library(Boost::${name} INTERFACE IMPORTED)
    set_target_properties(Boost::${name} PROPERTIES INTERFACE_INCLUDE_DIRECTORIES "${SOLC_ZK_BOOST}")
  endif()
  string(TOUPPER ${name} upper)
  set(Boost_${upper}_FOUND TRUE)
  set(Boost_${name}_FOUND TRUE)
endforeach()
