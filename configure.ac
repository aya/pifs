AC_INIT([pifs],[0.1],[support+C@asycn.io],[pifs])
AC_CONFIG_AUX_DIR(build)
AC_CONFIG_SRCDIR(src/pifs.c)
AC_CONFIG_HEADERS(config.h)
AM_INIT_AUTOMAKE

# Checks for programs
AC_PROG_CC

# Set language to C
AC_LANG([C])

# Checks for header files
AC_HEADER_DIRENT
AC_CHECK_INCLUDES_DEFAULT
AC_PROG_EGREP

PKG_CHECK_MODULES([FUSE], [fuse3 >= 3.1])

AC_SUBST([EXTRA_CFLAGS], ["-Wall -Werror -Wextra -Wno-unused-parameter"])

AC_CONFIG_FILES([Makefile src/Makefile])
AC_OUTPUT
