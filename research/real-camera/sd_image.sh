#!/bin/bash
# Read-only image of the first 64 MiB of a camera-formatted SD card, plus its
# partition/volume metadata and file listing. Usage: sd_image.sh disk4 OUTDIR
set -euo pipefail
disk=${1:?usage: sd_image.sh diskN OUTDIR}
out=${2:?usage: sd_image.sh diskN OUTDIR}
[[ $disk =~ ^disk[0-9]+$ ]] || { echo "need a whole-disk id like disk4"; exit 1; }
mkdir -p "$out"
diskutil info -plist "$disk" > "$out/diskutil-$disk.plist"
removable=$(plutil -extract RemovableMedia raw "$out/diskutil-$disk.plist" 2>/dev/null || echo false)
size=$(plutil -extract TotalSize raw "$out/diskutil-$disk.plist")
if [[ $removable != true ]] || (( size > 1099511627776 )); then
  echo "$disk is not removable media under 1 TiB; refusing."; exit 1
fi
diskutil list "$disk" > "$out/diskutil-list-$disk.txt"
for part in $(diskutil list "$disk" | awk '/^ +[0-9]+:/{print $NF}' | grep "^${disk}s"); do
  diskutil info "$part" > "$out/diskutil-$part.txt" || true
  mount=$(diskutil info "$part" | awk -F': *' '/Mount Point/{print $2}')
  [[ -n $mount && -d $mount ]] && (cd "$mount" && find . -ls) > "$out/files-$part.txt" || true
done
echo "Reading first 64 MiB of /dev/r$disk (read-only, needs your password)…"
sudo dd if="/dev/r$disk" of="$out/sd-first-64MiB.img" bs=1m count=64
shasum -a 256 "$out/sd-first-64MiB.img" | tee "$out/sd-first-64MiB.img.sha256"
