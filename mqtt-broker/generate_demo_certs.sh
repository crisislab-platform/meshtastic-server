dir=demo-certs
mkdir -p $dir
cd $dir

# ca
printf "\n====== CA ======\n\n"
openssl req -new -x509 -days 365 -extensions v3_ca -keyout ca.key -out ca.crt

# server
printf "\n====== Broker ======\n\n"
openssl genrsa -out broker.key 2048
openssl req -out broker.csr -key broker.key -new
openssl x509 -req -in broker.csr -CA ca.crt -CAkey ca.key -CAcreateserial -out broker.crt -days 365

function generate_client_cert {
	printf "\n====== Client ($1) ======\n\n"
	openssl genrsa -aes256 -out $1.key 2048
	openssl req -out $1.csr -key $1.key -new
	openssl x509 -req -in $1.csr -CA ca.crt -CAkey ca.key -CAcreateserial -out $1.crt -days 365
}

generate_client_cert client1
generate_client_cert client2
