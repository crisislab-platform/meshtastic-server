import mqtt from 'mqtt'
import * as protobuf from 'protobufjs'
import { question } from 'readline-sync'
import crypto from 'crypto'

if (!process.env.MQTT_USERNAME || !process.env.MQTT_PASSWORD) {
	throw new Error('MQTT_USERNAME and MQTT_PASSWORD must be set')
}

const client = mqtt.connect('mqtt://localhost', {
	username: process.env.MQTT_USERNAME,
	password: process.env.MQTT_PASSWORD,
})

const startupTime = Math.floor(Date.now() / 1000)

type Node = { node_num: number, user_id: string, short_name: string, long_name: string }
const nodes: Node[] = [];
for (let i = 0; i < 15; i++) {
	let node: Node = { node_num: 0, user_id: '', short_name: '', long_name: '' }
	do {
		node.node_num = Math.floor(Math.random() * 0xffffffff)
		node.user_id = crypto.randomBytes(16).toString('hex')
		node.short_name = Array.from(
			{ length: 4 },
			() => String.fromCharCode(65 + Math.floor(Math.random() * 26))
		).join('')
		node.long_name = `Dummy node ${i}`
	} while (nodes.some(n => n.node_num === node.node_num || n.short_name === node.short_name))
	nodes.push(node)
}

const randomLiveData = (HardwareModel, LocSource, AltSource) => {
	const node = nodes[Math.floor(Math.random() * nodes.length)]
	return {
		nodeNum: node.node_num,
		timestamp: Math.floor(Date.now() / 1000),
		user: {
			id: node.user_id,
			shortName: node.short_name,
			longName: node.long_name,
			hwModel: HardwareModel.values.TBEAM,
		},
		position: {
			latitudeI: Math.floor(Math.random() * 18000000) - 9000000, // -90 to 90 degrees
			longitudeI: Math.floor(Math.random() * 36000000) - 18000000, // -180 to 180 degrees
			altitude: Math.floor(Math.random() * 200), // Altitude in meters
			locationSource: LocSource.values.LOC_INTERNAL,
			altitudeSource: AltSource.values.ALT_INTERNAL,
			gpsAccuracy: 3000, // in mm
			groundSpeed: 0,
			satsInView: Math.floor(Math.random() * 7),
		},
		deviceMetrics: {
			batteryLevel: 101, // i.e. powered
			voltage: 5.3,
			channelUtilization: parseFloat((Math.random() * 5 + 2).toFixed(2)),
			airUtilTx: parseFloat(Math.random().toFixed(3)),
			uptimeSeconds: Math.floor(Date.now() / 1000) - startupTime,
		}
	}
}

protobuf.load("../../protobufs/bundle.json", async (error, root) => {
	if (error) throw error
	if (root === undefined) throw new Error("Root is undefined")

	const CrisislabMessage = root.lookupType("meshtastic.CrisislabMessage")
	const HardwareModel = root.lookup("meshtastic.HardwareModel")
	const LocSource = root.lookup("LocSource")
	const AltSource = root.lookup("AltSource")

	if (CrisislabMessage === null) throw new Error("CrisislabMessage is null")
	if (HardwareModel === null) throw new Error("HardwareModel is null")
	if (LocSource === null) throw new Error("LocSource is null")
	if (AltSource === null) throw new Error("AltSource is null")

	loop: while (true) {
		const option = question("Enter 's' to send signal data, 'l' to send live data, or 'q' to quit: ").toLowerCase()

		switch (option) {
			case 's':
				for (const packet of example_signal_data1) {
					const message = CrisislabMessage.create({
						signalData: packet
					})

					const buffer = CrisislabMessage.encode(message).finish()

					client.publish("for-server", Buffer.from(buffer))
				}
				break
			case 'l':
				console.log("Starting live data stream, use C-c to stop.")
				while (true) {
					const packet = randomLiveData(HardwareModel, LocSource, AltSource)
					console.log(packet)
					await new Promise(resolve => setTimeout(resolve, 1000))
				}
			case 'q':
				client.end()
				break loop
			default:
				console.log('Invalid option')
				break
		}
	}
})


const example_signal_data1 = [
	{
		to: 1,
		isGateway: true,
		links: [
			{ from: 2, rssi: -70, snr: 10 },
			{ from: 4, rssi: -20, snr: 10 },
		]
	},
	{
		to: 2,
		isGateway: false,
		links: [
			{ from: 1, rssi: -70, snr: 10 },
			{ from: 4, rssi: -20, snr: 10 },
			{ from: 5, rssi: -20, snr: 10 },
		]
	},
	{
		to: 3,
		isGateway: false,
		links: [
			{ from: 2, rssi: -60, snr: 10 },
			{ from: 5, rssi: -60, snr: 10 },
		]
	},
	{
		to: 4,
		isGateway: true,
		links: [
			{ from: 1, rssi: -20, snr: 10 },
			{ from: 5, rssi: -20, snr: 10 },
			{ from: 2, rssi: -30, snr: 10 },
		]
	},
	{
		to: 5,
		isGateway: false,
		links: [
			{ from: 4, rssi: -20, snr: 10 },
			{ from: 2, rssi: -30, snr: 10 },
			{ from: 3, rssi: -60, snr: 10 },
		]
	}
]

