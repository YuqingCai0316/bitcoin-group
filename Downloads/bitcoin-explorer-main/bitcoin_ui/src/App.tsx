import React, { useState, useEffect } from 'react';
import { BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts';
import axios from 'axios';

interface DataPoint {
    peer_count: number;
    medium_fee_per_kb: number;
    price: number;
    time: string;
}

function App() {
    const [data, setData] = useState<DataPoint[]>([]);

    // 获取初始数据
    useEffect(() => {
        async function fetchInitialData() {
            try {
                const response = await axios.get('http://34.44.123.189:3030/latest_blocks');
                const formattedData = response.data.map((item: any) => ({
                    ...item,
                    time: new Date().toLocaleTimeString(),
                }));
                setData(formattedData);
            } catch (error) {
                console.error('Failed to fetch initial data:', error);
            }
        }

        fetchInitialData();
    }, []);

    // WebSocket 连接
    useEffect(() => {
        const socket = new WebSocket('ws://34.44.123.189:3030/ws');

        socket.onmessage = (event) => {
            console.log("WebSocket message received: ", event.data);
            try {
                const parsedData = JSON.parse(event.data);
                const newDataPoint: DataPoint = {
                    ...parsedData,
                    time: new Date().toLocaleTimeString(),
                };

                setData((prevData) => {
                    const updatedData = [...prevData, newDataPoint];
                    // 保留最新的10条数据
                    return updatedData.slice(-10);
                });
            } catch (error) {
                console.error("Failed to parse WebSocket message:", error);
            }
        };

        socket.onopen = () => {
            console.log('WebSocket connection opened');
        };

        socket.onerror = (error) => {
            console.error('WebSocket error: ', error);
        };

        socket.onclose = (event) => {
            console.error('WebSocket connection closed:', event);
        };

        return () => {
            socket.close();
        };
    }, []);

    return (
        <div className="App">
            <h1 className="title">Our Bitcoin Explorer</h1>

            {/* Bar chart for price data */}
            {data.length > 0 ? (
                <ResponsiveContainer width="95%" height={400}>
                    <BarChart data={data}>
                        <CartesianGrid strokeDasharray="3 3" />
                        <XAxis dataKey="time" />
                        <YAxis />
                        <Tooltip />
                        <Bar dataKey="price" fill="#33ccff" />
                    </BarChart>
                </ResponsiveContainer>
            ) : (
                <p>Waiting for data...</p>
            )}

            {/* Table for detailed data */}
            <div style={{ marginTop: "20px" }}>
                <h2>Bitcoin Data Details</h2>
                <table>
                    <thead>
                        <tr>
                            <th>Time</th>
                            <th>Peer Count</th>
                            <th>Medium Fee per KB</th>
                            <th>Price</th>
                        </tr>
                    </thead>
                    <tbody>
                        {data.map((item, index) => (
                            <tr key={index}>
                                <td>{item.time}</td>
                                <td>{item.peer_count}</td>
                                <td>{item.medium_fee_per_kb}</td>
                                <td>{item.price}</td>
                            </tr>
                        ))}
                    </tbody>
                </table>
            </div>
        </div>
    );
}

export default App;
