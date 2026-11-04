// Boost.Filesystem operations for the zkVM, which has no files. Same results as Boost's own
// code for a path that does not exist (operations.cpp, status_impl). solc reaches status() only
// through util::readFileAsString, which the standard JSON path does not call.
#include <boost/filesystem/operations.hpp>

namespace boost::filesystem::detail
{
BOOST_FILESYSTEM_DECL file_status status(path const&, system::error_code* ec)
{
	if (ec)
		ec->clear();
	return file_status(file_not_found, no_perms);
}
}
