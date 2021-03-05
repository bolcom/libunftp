# run docker the same way it's used from gcs test

# docker expects absolute path
SCRIPTPATH="$(realpath $0)"
docker run --rm --name fake-gcs -v $(dirname $SCRIPTPATH)/data:/data -p 9081:9081 -it fsouza/fake-gcs-server -scheme http -port 9081

# keep in mind that fake-gcs writes data under `/storage`; it never pollutes `/data`!
# use `gcs_term.sh` to check out current contents (or use curl)
