@echo off
set RUSTC=%1
shift
"%RUSTC%" %*
