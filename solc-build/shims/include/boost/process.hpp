// Stand-in for <boost/process.hpp> in the zkVM, which cannot start programs.
// solc uses it only to find and run external SMT solvers (SMTSolverCommand.cpp,
// ModelChecker.cpp). search_path() finds nothing, so solc behaves exactly like a native
// solc on a machine without z3, cvc5 or eld in PATH. The other types only make the code compile.
#pragma once

#include <boost/filesystem/path.hpp>

#include <istream>
#include <ostream>
#include <stdexcept>
#include <string>

namespace boost::process
{
inline boost::filesystem::path search_path(std::string const&) { return {}; }

struct pipe_end { void close() {} };

struct opstream: std::ostream
{
	opstream(): std::ostream(nullptr) {}
	pipe_end& pipe() { return m_pipe; }
	void close() {}
	pipe_end m_pipe;
};

struct ipstream: std::istream
{
	ipstream(): std::istream(nullptr) {}
};

struct child
{
	template <class... Args>
	explicit child(Args&&...) { throw std::runtime_error("the zkVM cannot start programs"); }
	void wait() {}
};

struct redirect_out { template <class T> int operator>(T const&) const { return 0; } };
struct redirect_in { template <class T> int operator<(T const&) const { return 0; } };
struct null_device {};

inline constexpr redirect_out std_out{};
inline constexpr redirect_out std_err{};
inline constexpr redirect_in std_in{};
inline constexpr null_device null{};
}
