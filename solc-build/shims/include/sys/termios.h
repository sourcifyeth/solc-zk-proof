/* Stand-in for <sys/termios.h>, which newlib does not have for this target. solc uses it only
   in readStandardInputChar() of its command-line tool. The zkVM has no terminal: both calls fail. */
#pragma once

typedef unsigned int tcflag_t;
typedef unsigned char cc_t;
typedef unsigned int speed_t;

#define NCCS 32
struct termios
{
	tcflag_t c_iflag, c_oflag, c_cflag, c_lflag;
	cc_t c_cc[NCCS];
	speed_t c_ispeed, c_ospeed;
};

#define VTIME 5
#define VMIN 6
#define ICANON 0000002
#define ECHO 0000010
#define TCSANOW 0
#define TCSADRAIN 1

static inline int tcgetattr(int fd, struct termios* t) { (void)fd; (void)t; return -1; }
static inline int tcsetattr(int fd, int when, struct termios const* t) { (void)fd; (void)when; (void)t; return -1; }
