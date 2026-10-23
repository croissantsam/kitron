import React, { useEffect, useRef } from "react";
import { FlatList, Text, View } from "react-native";

import { app, logger } from "kitron";

const ITEMS = Array.from({ length: 2000 }, (_, i) => ({ key: String(i), title: `Row ${i}` }));

export default function BenchApp() {
  const listRef = useRef<any>(null);
  const startRef = useRef<number>(0);
  const reportedRef = useRef<boolean>(false);
  const offRef = useRef<number>(0);

  useEffect(() => {
    logger.info("ui mounted");
    try { app.onReady().then(() => logger.info("app.ready received")); } catch {}
    startRef.current = Date.now();
    const max = 2000 * 40;
    try { listRef.current?.scrollToOffset?.({ offset: max, animated: false }); } catch {}
    setTimeout(() => {
      if (!reportedRef.current) {
        reportedRef.current = true;
        logger.info("SCROLL_RESULT " + (Date.now() - startRef.current));
      }
    }, 0);
    const safety = setTimeout(() => {
      if (!reportedRef.current) {
        reportedRef.current = true;
        logger.info("SCROLL_RESULT " + (Date.now() - startRef.current));
      }
    }, 5000);
    return () => { clearTimeout(safety); };
  }, []);

  const onScroll = (e: any) => {
    if (reportedRef.current) return;
    const y = e?.nativeEvent?.contentOffset?.y ?? 0;
    const max = (e?.nativeEvent?.contentSize?.height ?? 0) - (e?.nativeEvent?.layoutMeasurement?.height ?? 0);
    if (max > 0 && y >= max - 50) {
      reportedRef.current = true;
      logger.info("SCROLL_RESULT " + (Date.now() - startRef.current));
    }
  };

  return (
    <FlatList
      ref={listRef}
      data={ITEMS}
      onScroll={onScroll}
      scrollEventThrottle={16}
      renderItem={({ item }: any) => (
        <View style={{ height: 40, padding: 8 }}>
          <Text>{item.title}</Text>
        </View>
      )}
      keyExtractor={(i: any) => i.key}
    />
  );
}
