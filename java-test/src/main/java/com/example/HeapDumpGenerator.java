package com.example;

import java.io.IOException;
import java.lang.management.ManagementFactory;
import java.util.*;
import javax.management.MBeanServer;
import com.sun.management.HotSpotDiagnosticMXBean;

public class HeapDumpGenerator {

    // Sample classes to populate the heap
    static class Person {
        String name;
        int age;
        List<String> hobbies;

        Person(String name, int age, String... hobbies) {
            this.name = name;
            this.age = age;
            this.hobbies = new ArrayList<>(Arrays.asList(hobbies));
        }
    }

    static class Company {
        String name;
        List<Person> employees;
        Map<String, Integer> departments;

        Company(String name) {
            this.name = name;
            this.employees = new ArrayList<>();
            this.departments = new HashMap<>();
        }

        void addEmployee(Person person) {
            employees.add(person);
        }

        void addDepartment(String dept, int count) {
            departments.put(dept, count);
        }
    }

    static class DataBuffer {
        byte[] data;

        DataBuffer(int size) {
            this.data = new byte[size];
            new Random().nextBytes(data);
        }
    }

    private static List<Object> keepAlive = new ArrayList<>();

    public static void main(String[] args) throws IOException {
        System.out.println("Generating heap dump with sample data...");

        // Create various objects to make the heap dump interesting
        Company company1 = new Company("TechCorp");
        company1.addDepartment("Engineering", 50);
        company1.addDepartment("Sales", 30);
        company1.addDepartment("Marketing", 20);

        for (int i = 0; i < 100; i++) {
            Person person = new Person(
                "Employee" + i,
                25 + (i % 40),
                "Reading", "Coding", "Gaming"
            );
            company1.addEmployee(person);
        }

        Company company2 = new Company("DataSoft");
        company2.addDepartment("Engineering", 80);
        company2.addDepartment("Research", 40);

        for (int i = 0; i < 150; i++) {
            Person person = new Person(
                "Engineer" + i,
                22 + (i % 35),
                "Music", "Sports"
            );
            company2.addEmployee(person);
        }

        // Create some large buffers
        for (int i = 0; i < 10; i++) {
            keepAlive.add(new DataBuffer(1024 * 100)); // 100KB each
        }

        // Create lots of strings
        Set<String> stringPool = new HashSet<>();
        for (int i = 0; i < 1000; i++) {
            stringPool.add("String_" + i + "_" + UUID.randomUUID());
        }

        // Create various collection types
        Map<String, List<Integer>> complexMap = new HashMap<>();
        for (int i = 0; i < 50; i++) {
            List<Integer> numbers = new ArrayList<>();
            for (int j = 0; j < 20; j++) {
                numbers.add(j * i);
            }
            complexMap.put("Key" + i, numbers);
        }

        // Keep references to prevent GC
        keepAlive.add(company1);
        keepAlive.add(company2);
        keepAlive.add(stringPool);
        keepAlive.add(complexMap);

        System.out.println("Created objects:");
        System.out.println("  - 2 companies with " +
            (company1.employees.size() + company2.employees.size()) + " total employees");
        System.out.println("  - " + stringPool.size() + " unique strings");
        System.out.println("  - 10 data buffers (1MB total)");
        System.out.println("  - Complex map with 50 entries");

        // Generate heap dump
        String filename = "heap-dump.hprof";
        System.out.println("\nGenerating heap dump to: " + filename);
        dumpHeap(filename, true);
        System.out.println("Heap dump generated successfully!");

        System.out.println("\nYou can now analyze this dump with the Rust parser.");
    }

    private static void dumpHeap(String filename, boolean live) throws IOException {
        MBeanServer server = ManagementFactory.getPlatformMBeanServer();
        HotSpotDiagnosticMXBean mxBean = ManagementFactory.newPlatformMXBeanProxy(
            server,
            "com.sun.management:type=HotSpotDiagnostic",
            HotSpotDiagnosticMXBean.class
        );
        mxBean.dumpHeap(filename, live);
    }
}
