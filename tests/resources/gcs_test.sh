# run docker the same way it's used from gcs test
docker run --rm --name fake-gcs -v `pwd`/tests/resources/data:/data -p 9081:9081 -it fsouza/fake-gcs-server -scheme http -port 9081
