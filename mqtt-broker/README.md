# MQTT Broker

## Setup

1. Install Mosquitto and OpenSSL (which you'll almost certainly have already but for some reason on MacOS I need the nixpkgs version for this). On Nix:

    ```
    nix develop .#mqtt
    ```

2. Create password file for server and gateway users:

    ```
    mosquitto_passwd -c passwords.txt server
    mosquitto_passwd passwords.txt gateway
    ```

3. Set cert files accordingly in `mosquitto.conf`, or for testing purposes, generate certs by running `./generate-certs.sh` from this directory. When entering info, ensure that the "Common Name" is your hostname or "localhost", and that some properties differ between certs.

4. Test that everything works by running each of the following in a different terminal:

    1. Start the broker:

        ```
        mosquitto -c mosquitto.conf
        ```

    2. Subscribe to a topic as the server (adjust cert file paths as necessary):

        ```
        mosquitto_sub -t data -u server -P <PASSWORD> --cafile demo-certs/ca.crt --cert demo-certs/client1.crt --key demo-certs/client1.key
        ```

    3. Publish a message to that same topic as a gateway:

    ```
    mosquitto_pub -t data -u gateway -P <PASSWORD> --cafile demo-certs/ca.crt --cert demo-certs/client2.crt --key demo-certs/client2.key -m "Hello"
    ```

    You should see "Hello" appear in the pane running `mosquitto_sub`.
