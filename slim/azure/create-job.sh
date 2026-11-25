#!/bin/sh
# Create (or update) an Azure Container Apps job that clones a repository, indexes it with
# the headless ax image and runs a command. Needs the Azure CLI (`az login` first) with the
# containerapp extension.
#
#   slim/azure/create-job.sh --resource-group my-rg --environment my-env \
#     --repo https://github.com/owner/repo --version v10.1.1 --start
#
# Options:
#   --resource-group RG   resource group (required)
#   --environment ENV     Container Apps environment (required; created when --location is given)
#   --location LOC        create the environment in this region if it does not exist
#   --repo URL            git URL to index (required; stored as a job secret)
#   --ref REF             branch or tag to index
#   --command CMD         command to run after indexing (default: ax status)
#   --name NAME           job name (default: ax-index)
#   --version vX.Y.Z      ax release; the image is ghcr.io/garywenneker/ax-slim:<X.Y.Z>-ubuntu
#   --image IMAGE         full image reference instead of --version
#   --cpu N / --memory M  container size (default 1.0 / 2.0Gi)
#   --start               start one execution right away and print the log command
set -eu

NAME="ax-index"; RG=""; ENV_NAME=""; LOCATION=""; REPO=""; REF=""; CMD="ax status"
VERSION=""; IMAGE=""; CPU="1.0"; MEMORY="2.0Gi"; START=0

die() { printf 'create-job: %s\n' "$*" >&2; exit 1; }
need() { [ $# -ge 2 ] || die "$1 needs a value"; }

while [ $# -gt 0 ]; do
  case "$1" in
    --resource-group|-g) need "$@"; RG="$2"; shift 2 ;;
    --environment) need "$@"; ENV_NAME="$2"; shift 2 ;;
    --location) need "$@"; LOCATION="$2"; shift 2 ;;
    --repo) need "$@"; REPO="$2"; shift 2 ;;
    --ref) need "$@"; REF="$2"; shift 2 ;;
    --command) need "$@"; CMD="$2"; shift 2 ;;
    --name|-n) need "$@"; NAME="$2"; shift 2 ;;
    --version) need "$@"; VERSION="$2"; shift 2 ;;
    --image) need "$@"; IMAGE="$2"; shift 2 ;;
    --cpu) need "$@"; CPU="$2"; shift 2 ;;
    --memory) need "$@"; MEMORY="$2"; shift 2 ;;
    --start) START=1; shift ;;
    -h|--help) sed -n '2,23p' "$0"; exit 0 ;;
    *) die "unknown option '$1' (see --help)" ;;
  esac
done

[ -n "$RG" ] || die "--resource-group is required"
[ -n "$ENV_NAME" ] || die "--environment is required"
[ -n "$REPO" ] || die "--repo is required"
command -v az >/dev/null 2>&1 || die "the Azure CLI (az) is required"
if [ -z "$IMAGE" ]; then
  [ -n "$VERSION" ] || die "pass --version vX.Y.Z (a release with headless assets) or --image"
  IMAGE="ghcr.io/garywenneker/ax-slim:${VERSION#v}-ubuntu"
fi

if ! az containerapp env show --name "$ENV_NAME" --resource-group "$RG" --output none 2>/dev/null; then
  [ -n "$LOCATION" ] || die "environment '$ENV_NAME' not found in '$RG'; pass --location to create it"
  echo "Creating Container Apps environment $ENV_NAME in $LOCATION" >&2
  az containerapp env create --name "$ENV_NAME" --resource-group "$RG" --location "$LOCATION" --output none
fi

set -- --name "$NAME" --resource-group "$RG" \
  --image "$IMAGE" --cpu "$CPU" --memory "$MEMORY" \
  --command "ax-index-repo" \
  --secrets "ax-repo-url=$REPO" \
  --env-vars "AX_REPO_URL=secretref:ax-repo-url" "AX_JOB_COMMAND=$CMD" ${REF:+"AX_REPO_REF=$REF"} \
  --output none

if az containerapp job show --name "$NAME" --resource-group "$RG" --output none 2>/dev/null; then
  echo "Updating job $NAME ($IMAGE)" >&2
  az containerapp job secret set --name "$NAME" --resource-group "$RG" --secrets "ax-repo-url=$REPO" --output none
  az containerapp job update --name "$NAME" --resource-group "$RG" \
    --image "$IMAGE" --cpu "$CPU" --memory "$MEMORY" --command "ax-index-repo" \
    --replace-env-vars "AX_REPO_URL=secretref:ax-repo-url" "AX_JOB_COMMAND=$CMD" ${REF:+"AX_REPO_REF=$REF"} \
    --output none
else
  echo "Creating job $NAME ($IMAGE)" >&2
  az containerapp job create "$@" --environment "$ENV_NAME" --container-name "$NAME" \
    --trigger-type Manual --replica-timeout 3600 --replica-retry-limit 0 \
    --replica-completion-count 1 --parallelism 1
fi

if [ "$START" = 1 ]; then
  az containerapp job start --name "$NAME" --resource-group "$RG" --output none
  echo "Started. Follow the output with:" >&2
fi
echo "az containerapp job logs show --name $NAME --resource-group $RG --container $NAME --follow"
