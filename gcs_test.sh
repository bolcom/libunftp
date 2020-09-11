docker run --rm --name fake-gcs --mount type=bind,src=`pwd`/data,dst=/data -p 9081:9081 -it fsouza/fake-gcs-server -scheme http -port 9081
