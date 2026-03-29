#!/usr/bin/env bash
# Switch Gathering mount from SMB to NFS
# Run on Library Mac (primary). Requires sudo password for both machines.
set -euo pipefail

BEDROOM=192.168.86.242
LIBRARY=192.168.86.36
EXPORT_PATH="/Volumes/VideosNew/Gathering"
MOUNT_POINT="/Volumes/Gathering"

echo "=== Step 1: Add NFS export on Bedroom Mac ==="
ssh -t jeffbridwell@${BEDROOM} "echo '${EXPORT_PATH} -alldirs -mapall=jeffbridwell ${LIBRARY}' | sudo tee /etc/exports && sudo nfsd enable && sudo nfsd restart"

echo ""
echo "=== Step 2: Verify Bedroom NFS export ==="
ssh jeffbridwell@${BEDROOM} "showmount -e localhost"

echo ""
echo "=== Step 3: Unmount SMB on Library ==="
umount "${MOUNT_POINT}" 2>/dev/null || diskutil unmount "${MOUNT_POINT}" 2>/dev/null || echo "Already unmounted"

echo ""
echo "=== Step 4: Mount NFS on Library ==="
sudo mount_nfs -o resvport ${BEDROOM}:${EXPORT_PATH} "${MOUNT_POINT}"

echo ""
echo "=== Step 5: Verify ==="
mount | grep Gathering
echo ""
echo "Files visible:"
ls "${MOUNT_POINT}/Music/" | head -5
echo "..."
echo ""
echo "Done. Mount is NFS. Apple Music should now scan properly."
echo "NOTE: This mount won't survive a reboot. Run 'sudo vifs' to add to /etc/fstab for persistence."
