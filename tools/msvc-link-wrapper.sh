#!/usr/bin/env bash
set -euo pipefail

# Translate MSVC-style linker flags produced by rustc for the i686-win7-windows-msvc
# target into MinGW-compatible arguments and delegate to the host MinGW linker.
target_linker="i686-w64-mingw32-gcc"
extra_lib_paths=("/usr/i686-w64-mingw32/lib")
fallback_libs=("-lgcc" "-lgcc_eh" "-lmingwex" "-lmingw32" "-lmsvcrt")

args=()
for raw in "$@"; do
  case "$raw" in
    /DEF:*)
      # The generated .def file is optional for our MinGW linker path, so skip it.
      ;;
    /OUT:*)
      args+=("-o" "${raw#/OUT:}")
      ;;
    /IMPLIB:*)
      args+=("-Wl,--out-implib,${raw#/IMPLIB:}")
      ;;
    /LIBPATH:*)
      args+=("-L${raw#/LIBPATH:}")
      ;;
    /DEFAULTLIB:*|/defaultlib:*)
      libname=${raw#/DEFAULTLIB:}
      libname=${libname#/defaultlib:}
      libname=${libname%.lib}
      args+=("-l${libname}")
      ;;
    /LARGEADDRESSAWARE)
      args+=("-Wl,--large-address-aware")
      ;;
    /NXCOMPAT)
      args+=("-Wl,--nxcompat")
      ;;
    /DLL)
      args+=("-shared")
      ;;
    /NOLOGO|/SAFESEH|/DEBUG|/PDBALTPATH:*)
      # These flags are specific to the MSVC toolchain and do not have
      # meaningful MinGW equivalents for our build.
      ;;
    /OPT:*)
      # Ignore optional MSVC optimizations; the MinGW linker will perform its own.
      ;;
    *.lib)
      # Preserve explicit library file references.
      args+=("-l:${raw##*/}")
      ;;
    *)
      args+=("${raw}")
      ;;
  esac
done

for path in "${extra_lib_paths[@]}"; do
  args=("-L" "${path}" "${args[@]}")
done

exec "${target_linker}" "${args[@]}" "${fallback_libs[@]}"
