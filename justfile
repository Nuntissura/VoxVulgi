set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

default:
  @just --list

vv-start:
  @powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File .\governance\scripts\vv_start.ps1
