FROM alpine:latest

RUN apk add bash openssl nettle-utils jq pwgen
COPY files/run.sh /

ENTRYPOINT ["/run.sh"]
